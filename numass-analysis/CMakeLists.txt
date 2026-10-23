cmake_minimum_required(VERSION 3.24)

project(numass-analysis-scripts VERSION 3.1.0 LANGUAGES CXX)

# --- Общие настройки --------------------------------------------------------

# compile_commands.json нужен clangd (Zed) для навигации по ROOT-заголовкам
set(CMAKE_EXPORT_COMPILE_COMMANDS ON)

# Все бинарники складываются в build/bin
set(CMAKE_RUNTIME_OUTPUT_DIRECTORY "${CMAKE_BINARY_DIR}/bin")

# Скрипты — это ROOT-макросы, а не программы, поэтому main() генерируется
# (см. cmake/script-main.hxx). По умолчанию собираем с оптимизацией.
if(NOT CMAKE_BUILD_TYPE AND NOT CMAKE_CONFIGURATION_TYPES)
    set(CMAKE_BUILD_TYPE Release CACHE STRING "Build type" FORCE)
endif()

# --- ROOT -------------------------------------------------------------------
# Включает Core, RIO, Tree, RDataFrame, ROOTVecOps, Graf, Hist, Gpad, MathCore
# и остальные библиотеки, перечисленные в ROOTConfig.cmake.
find_package(ROOT 6.16 CONFIG REQUIRED)

# ROOT собран с конкретным стандартом C++ — используем тот же, чтобы не ловить
# предупреждение о несовпадении ABI-настроек из RConfigure.h.
if(ROOT_CXX_STANDARD)
    set(CMAKE_CXX_STANDARD ${ROOT_CXX_STANDARD})
else()
    set(CMAKE_CXX_STANDARD 20)
endif()
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)

# --- CPM — пакетный менеджер ------------------------------------------------
# Сам CPM скачивается при конфигурации (с проверкой SHA256), в репозитории
# его код не хранится.
set(CPM_DOWNLOAD_VERSION 0.43.2)
set(CPM_DOWNLOAD_LOCATION "${CMAKE_BINARY_DIR}/cmake/CPM_${CPM_DOWNLOAD_VERSION}.cmake")

if(NOT EXISTS "${CPM_DOWNLOAD_LOCATION}")
    message(STATUS "CPM: скачивание v${CPM_DOWNLOAD_VERSION}")
    file(DOWNLOAD
        "https://github.com/cpm-cmake/CPM.cmake/releases/download/v${CPM_DOWNLOAD_VERSION}/CPM.cmake"
        "${CPM_DOWNLOAD_LOCATION}"
        EXPECTED_HASH SHA256=49a3bef91ceb65bb66d57255e12d1ffd22abc2f6408fa9fe4534c544a2f232aa
        TLS_VERIFY ON
    )
endif()
include("${CPM_DOWNLOAD_LOCATION}")

# CLI11 — разбор аргументов командной строки в скриптах
CPMAddPackage("gh:CLIUtils/CLI11@2.4.2")

# nlohmann_json — JSON (используется, например, в scripts/recalibration/calibrate.C)
CPMAddPackage("gh:nlohmann/json@3.12.0")

# --- Бинарники из scripts/**.C ---------------------------------------------

file(GLOB_RECURSE NUMASS_SCRIPTS CONFIGURE_DEPENDS
    "${CMAKE_CURRENT_SOURCE_DIR}/scripts/*.C"
)

set(NUMASS_SCRIPT_MAIN_HEADER "${CMAKE_CURRENT_SOURCE_DIR}/cmake/script-main.hxx")
set(NUMASS_SCRIPT_GENERATED_DIR "${CMAKE_BINARY_DIR}/generated")
file(MAKE_DIRECTORY "${NUMASS_SCRIPT_GENERATED_DIR}")

set(NUMASS_SCRIPT_LIBS
    ${ROOT_LIBRARIES}
    nlohmann_json::nlohmann_json
    CLI11::CLI11
)

set(NUMASS_SCRIPT_NAMES "")

foreach(script IN LISTS NUMASS_SCRIPTS)
    get_filename_component(script_name "${script}" NAME_WE)
    # fit-deltas.C -> fit_deltas
    string(MAKE_C_IDENTIFIER "${script_name}" entry_point)

    if(script_name IN_LIST NUMASS_SCRIPT_NAMES)
        message(FATAL_ERROR
            "scripts/: несколько файлов с именем ${script_name}.C — "
            "имена бинарников должны быть уникальны"
        )
    endif()
    list(APPEND NUMASS_SCRIPT_NAMES "${script_name}")

    file(TO_CMAKE_PATH "${script}" script_path)
    file(TO_CMAKE_PATH "${NUMASS_SCRIPT_MAIN_HEADER}" main_header_path)

    set(main_source "${NUMASS_SCRIPT_GENERATED_DIR}/${script_name}-main.cxx")
    file(CONFIGURE OUTPUT "${main_source}" @ONLY CONTENT [=[
// Сгенерировано CMake, не редактировать вручную.
#include <nlohmann/json.hpp>

#include "@script_path@"
#include "@main_header_path@"

NUMASS_SCRIPT_MAIN(@entry_point@)
]=])

    add_executable(${script_name} "${main_source}")
    target_include_directories(${script_name} PRIVATE ${ROOT_INCLUDE_DIRS})
    target_link_libraries(${script_name} PRIVATE ${NUMASS_SCRIPT_LIBS})

    message(STATUS "scripts: ${script_name} <- ${script}")
endforeach()

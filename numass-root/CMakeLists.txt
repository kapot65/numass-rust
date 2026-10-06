cmake_minimum_required(VERSION 3.20)
project(numass-root VERSION 0.1.0 LANGUAGES C CXX)

# Собирать с -O3 -march=native (отключить: -DENABLE_NATIVE_OPTIMIZATION=OFF)
option(ENABLE_NATIVE_OPTIMIZATION "Enable -O3 -march=native" ON)

# По умолчанию собираем Release (иначе Corrosion соберёт Rust в dev-профиле)
if(NOT CMAKE_BUILD_TYPE AND NOT CMAKE_CONFIGURATION_TYPES)
  set(CMAKE_BUILD_TYPE Release CACHE STRING "Build type" FORCE)
endif()

# Генерировать build/compile_commands.json для clangd (навигация по C++/ROOT)
set(CMAKE_EXPORT_COMPILE_COMMANDS ON)

# --- Зависимости через CPM ----------------------------------------------
include(cmake/CPM.cmake)

# Rust-интеграция: собирает кран и генерирует bindings/processing-internal.h
CPMAddPackage(
    NAME corrosion
    GIT_REPOSITORY https://github.com/corrosion-rs/corrosion.git
    GIT_TAG v0.5.1
)

message(${CMAKE_CURRENT_SOURCE_DIR})
corrosion_import_crate(
    MANIFEST_PATH ${CMAKE_CURRENT_SOURCE_DIR}/../Cargo.toml
    CRATES numass-root
)

# argparse — header-only парсер аргументов
CPMAddPackage(
    NAME argparse
    GIT_REPOSITORY https://github.com/p-ranav/argparse.git
    GIT_TAG v3.2
)

# --- ROOT --------------------------------------------------------------
# find_package(ROOT) требует Vdt (зависимость ROOT). Если её нет:
#   sudo apt-get install libvdt-dev
# или соберите Vdt локально и передайте -DCMAKE_PREFIX_PATH=/path/to/vdt
find_package(ROOT 6.16 CONFIG REQUIRED)

# --- Исполняемый файл ---------------------------------------------------
add_executable(convert-to-root convert-to-root.cxx)
target_compile_features(convert-to-root PRIVATE cxx_std_17)

target_include_directories(convert-to-root PRIVATE
    bindings
    ${argparse_SOURCE_DIR}/include
)

if(ENABLE_NATIVE_OPTIMIZATION)
  target_compile_options(convert-to-root PRIVATE -O3 -march=native)
endif()

target_link_libraries(
    convert-to-root
    PRIVATE
    ROOT::Core
    ROOT::Tree
    numass_root
)

cmake_minimum_required(VERSION 3.5.0)
project(numass-root VERSION 0.1.0 LANGUAGES C CXX)

include(cmake/CPM.cmake)

include(FetchContent)

# Подключение rust к cmake
FetchContent_Declare(
    Corrosion
    GIT_REPOSITORY https://github.com/corrosion-rs/corrosion.git
    GIT_TAG v0.5.1 # Optionally specify a commit hash, version tag or branch here
)
FetchContent_MakeAvailable(Corrosion)

# Сборка rust зависимостей
# set(CORROSION_RUST_TARGET "x86_64-unknown-linux-gnu")
set(CORROSION_CRATE_NAME "numass_root")
corrosion_import_crate(MANIFEST_PATH ./Cargo.toml)

# Подключение c++ зависимостей
CPMAddPackage("gh:p-ranav/argparse@3.2")
include_directories(${argparse_SOURCE_DIR}/include)


find_package(ROOT 6.16 CONFIG REQUIRED)

include_directories(${ROOT_INCLUDE_DIRS})
include_directories(bindings)
link_directories(${ROOT_LIBRARY_DIR})


add_executable(convert-to-root convert-to-root.cxx)
# target_link_directories(convert-to-root PRIVATE "${CMAKE_SOURCE_DIR}/target/release")
target_link_libraries(
    convert-to-root 
    ROOT::Core # `root-config --libs` to list available libraries
    ROOT::Tree 
    numass_root
)

cmake_minimum_required(VERSION 3.5.0)
project(numass-root VERSION 0.1.0 LANGUAGES C CXX)

include(cmake/CPM.cmake)

CPMAddPackage("gh:p-ranav/argparse@3.2")

find_package(ROOT 6.16 CONFIG REQUIRED)

include_directories(${ROOT_INCLUDE_DIRS})
include_directories(bindings)
include_directories(${argparse_SOURCE_DIR}/include)
link_directories(${ROOT_LIBRARY_DIR}) 


add_executable(convert-to-root convert-to-root.cxx)
target_link_directories(convert-to-root PRIVATE "${CMAKE_SOURCE_DIR}/target/release")
target_link_libraries(
    convert-to-root 
    ROOT::Core # `root-config --libs` to list available libraries
    ROOT::Tree 
    numass_root
)

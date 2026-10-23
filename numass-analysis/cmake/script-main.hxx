#pragma once

#include <cstdlib>
#include <iostream>
#include <type_traits>

// Точка входа для ROOT-макроса из scripts/.
//
// CMake генерирует для каждого scripts/**/*.C файл с main(), который вызывает
// функцию с тем же именем, что и файл (fit-deltas.C -> fit_deltas()).
// Поддерживаются две формы функции:
//   * без аргументов — вызывается как есть;
//   * (const char* folder, double left, double right) — аргументы берутся
//     из командной строки как <folder> <left> <right>.
template <class Func>
int numass_run_script(int argc, char** argv, Func func) {
    if constexpr (std::is_invocable_v<Func>) {
        func();
    } else if constexpr (std::is_invocable_v<Func, const char*, double, double>) {
        if (argc < 4) {
            std::cerr << "usage: " << argv[0] << " <folder> <left> <right>\n";
            return 1;
        }
        func(argv[1], std::strtod(argv[2], nullptr), std::strtod(argv[3], nullptr));
    } else {
        std::cerr << "unsupported script signature\n";
        return 1;
    }
    return 0;
}

#define NUMASS_SCRIPT_MAIN(FUNC)                     \
    int main(int argc, char** argv) {                \
        return numass_run_script(argc, argv, &FUNC); \
    }

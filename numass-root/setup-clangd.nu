#!/usr/bin/env nu

# Настраивает clangd для numass-root.
#
# clangd узнаёт include-пути CERN ROOT (root-config --incdir), заголовки
# bindings/ и argparse из build/compile_commands.json, который генерирует CMake
# (в CMakeLists.txt включён CMAKE_EXPORT_COMPILE_COMMANDS). Файл .clangd в корне
# проекта указывает clangd на каталог build/.

def main [] {
    # 1. ROOT должен быть доступен через root-config.
    if (which root-config | is-empty) {
        print "Ошибка: root-config не найден в PATH."
        print "Установите CERN ROOT и добавьте его в PATH."
        exit 1
    }

    let incdir = (^root-config --incdir | str trim)
    print $"ROOT include: ($incdir)"

    # 2. (Пере)генерируем build/compile_commands.json.
    print "Генерация build/compile_commands.json ..."
    cmake -S . -B build -DCMAKE_EXPORT_COMPILE_COMMANDS=ON

    if ("build/compile_commands.json" | path exists) {
        print "Готово: build/compile_commands.json создан."
        print "Перезапустите языковой сервер в Zed (language server: restart)."
    } else {
        print "Предупреждение: build/compile_commands.json не найден после cmake."
    }
}

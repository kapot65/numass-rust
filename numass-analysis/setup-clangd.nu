#!/usr/bin/env nu

# Настраивает clangd для C++/ROOT-скриптов numass-analysis.
#
# Перегенерирует build/compile_commands.json из CMakeLists.txt (там включён
# CMAKE_EXPORT_COMPILE_COMMANDS). Отдельный .clangd не нужен: clangd ищет
# compile_commands.json в родительских каталогах исходника и в их подкаталогах
# build/, а build/ лежит прямо в numass-analysis.
#
# Запускать можно из любого каталога: скрипт сам переходит в numass-analysis.

const SCRIPT_DIR = path self .

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
    cd $SCRIPT_DIR
    print "Генерация build/compile_commands.json ..."
    cmake -S . -B build -DCMAKE_EXPORT_COMPILE_COMMANDS=ON

    if ("build/compile_commands.json" | path exists) {
        print "Готово: build/compile_commands.json создан."
        print "Перезапустите языковой сервер в Zed (language server: restart)."
    } else {
        print "Предупреждение: build/compile_commands.json не найден после cmake."
    }
}

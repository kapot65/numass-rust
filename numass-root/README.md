# numass-root
Бинды для CERN ROOT (C++) алгоритмов из numass-processing


## Сборка скриптов

- [convert-to-root.C](convert-to-root.C) собирается командой:
    ```bash
    g++ -o convert-to-root convert-to-root.C `root-config --cflags --glibs` -O3 -march=native -fPIC
    ```
    другие бинарники root собираются аналогично.



## Разное
- скачать последнюю версию CPM
  ```bash
  wget -O cmake/CPM.cmake https://github.com/cpm-cmake/CPM.cmake/releases/latest/download/get_cpm.cmake
  ```

// Test bindings by reading point from a file.
#include "../bindings/processing.h"
#include <TTree.h>
#include <iostream>
#include <string>

using namespace std;

void get_meta() {
    auto path = string("/data-fast/numass-server/2024_11/Tritium_2_1/set_1/p0(30s)(HV1=14000)");
    auto meta = get_meta(std::move(path));

    cout << meta << endl;
}
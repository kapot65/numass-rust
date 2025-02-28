#include "TSystem.h"
#include <iostream>
using namespace std;

void rootlogon() {
    gSystem->Load("../numass-root/target/release/libnumass_root.so");
    cout << "numass-root loaded successfully!" << endl;
}

#include "../numass-root/bindings/processing.h"
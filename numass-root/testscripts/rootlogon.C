#include <TSystem.h>
#include <iostream>
using namespace std;

void rootlogon() {
    gSystem->Exec("cargo build --release");
    gSystem->Load("../target/release/libnumass_root.so");

    cout << "Welcome to ROOT!" << endl;
}

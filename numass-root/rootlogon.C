gSystem->Exec("cargo build --release");
gSystem->Load("./target/release/libnumass_root.so");
#include "bindings.h"

void rootlogon() {
    cout << "Welcome to ROOT!" << endl;
}

void test_callback(const char* str) {
    cout << "Callback received: " << str << endl;
}

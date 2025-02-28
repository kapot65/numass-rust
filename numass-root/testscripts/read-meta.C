// test-converter.sh must be run before this script.
#include <TFile.h>
#include <TString.h>
#include <iostream>

using namespace std;

void read_meta() {

  auto file = TFile::Open("point.root");
  if (!file || file->IsZombie()) {
    std::cerr << "Failed to open file!" << std::endl;
    return;
  }

  // from [example](https://root.cern/manual/root_files/#reading-an-object-from-a-root-file)
  std::unique_ptr<TString> meta(file->Get<TString>("meta"));
  if (!meta) {
    std::cerr << "Failed to find meta tree!" << std::endl;
    return;
  }

  cout << "Meta data: " << *meta << endl;
}
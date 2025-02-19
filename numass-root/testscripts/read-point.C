// Test bindings by reading point from a file.
#include "../bindings/processing.h"
#include <TTree.h>
#include <string>

using namespace std;

TTree* tree = nullptr;

auto test_read_point() {

  auto path = string("/data-fast/numass-server/2024_11/Tritium_2_1/set_1/p0(30s)(HV1=14000)");
  auto params = get_process_default();

  tree = process(path, &params);
  // tree->Draw("amplitude");
  return tree;
}
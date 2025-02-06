#include <cstdint>
#include <functional>
#include <string>

#include "RtypesCore.h"
#include "TTree.h"
#include "bindings.h"

// Define the variables for the branches
ULong64_t curr_time;
UChar_t curr_channel;
Float_t curr_amplitude;

TTree *curr_tree = nullptr;

// struct ProcessedTree {
//     ULong64_t curr_time;
//     UChar_t curr_channel;
//     Float_t curr_amplitude;
//     TTree *curr_tree = nullptr;
// }

void fill_curr_tree(uint64_t t, uint8_t c, float a) {
    curr_time = t;
    curr_channel = c;
    curr_amplitude = a;
    curr_tree->Fill();
}

auto test_read_point() {

  auto path = std::string(
      "/data-fast/numass-server/2024_11/Tritium_2_1/set_1/p0(30s)(HV1=14000)");
  auto algo = get_trapeziod_default();

  // Create a tree with name "T" and title "Example Tree"
  TTree *tree = new TTree("T", "Example Tree");
  curr_tree = tree; // Assign the created tree to curr_tree

  // Create branches for the tree
  tree->Branch("time", &curr_time, "time/l"); // 'l' stands for ULong64_t
  tree->Branch(
      "channel", &curr_channel,
      "channel/b"); // 'b' stands for Bool_t or Char_t (UChar_t in this case)
  tree->Branch("amplitude", &curr_amplitude,
               "amplitude/F"); // 'F' stands for Float_t

  process_point(&path, &algo, fill_curr_tree);
  curr_tree = nullptr; // Reset curr_tree to nullptr after processing is done

  return tree;
}
#ifndef processing_h
#define processing_h

#include <TTree.h>
#include <cstdint>
#include "processing-internal.h"

static void fill_tree(ProcessedTree* tree, uint64_t t, uint8_t c, float a, uint16_t s) {
    tree->time = t;
    tree->channel = c;
    tree->amplitude = a;
    tree->size = s;
    reinterpret_cast<TTree*>(tree->tree_ref)->Fill();
}

inline TTree* process(std::string path, ProcessParams *params, PostProcessParams *postprocess=nullptr) {
    // Create a tree with name "T" and title "Example Tree"
    TTree *tree = new TTree("T", "Example Tree");

    auto curr = ProcessedTree {
        .tree_ref = reinterpret_cast<size_t>(tree),
    };

    // Create branches for the tree
    tree->Branch("time", &curr.time, "time/l"); // 'l' stands for ULong64_t
    tree->Branch(
        "channel", &curr.channel,
        "channel/b"); // 'b' stands for Bool_t or Char_t (UChar_t in this case)
    tree->Branch("amplitude", &curr.amplitude,
                "amplitude/F"); // 'F' stands for Float_t
    tree->Branch("size", &curr.size,
                "size/s");

    process_point(
        &path, 
        &curr,
        fill_tree,
        params,
        nullptr
    );

    return tree;
}

#endif  // processing_h

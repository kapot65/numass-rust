/// High-level bindings for processing data in C++.
#ifndef processing_h
#define processing_h

#include <TTree.h>
#include <cstddef>
#include <cstdint>
#include <string>
#include "processing-internal.h"

static void fill_tree(ProcessedTree* tree, uint64_t t, uint8_t c, float a, uint16_t s) {
    tree->time = t;
    tree->channel = c;
    tree->amplitude = a;
    tree->size = s;
    reinterpret_cast<TTree*>(tree->tree_ref)->Fill();
}

const uint64_t BUFFER_SIZE = 1024 * 1024; // 1MB buffer

inline std::string get_meta(std::string path) {
    uint8_t buffer[BUFFER_SIZE];
    size_t len = read_meta_c(&path, buffer, BUFFER_SIZE);

    return std::string(reinterpret_cast<char*>(buffer), len); // Convert buffer to string
}

inline TTree* process(std::string path, ProcessParams *params, PostProcessParams *postprocess=nullptr) {
    

    TTree *tree = new TTree("T", "Events");

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

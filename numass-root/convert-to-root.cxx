#include <iostream>
#include <filesystem>

#include <TFile.h>
#include <TApplication.h>

#include <TMacro.h>
#include <TObjString.h>
#include <TString.h>
#include "processing.h"

#include "argparse/argparse.hpp"


int main(int argc, char** argv) {

    argparse::ArgumentParser program("convert-to-root");

    program.add_argument("path")
        .help("Path to point");

    program.add_argument("-p", "--process")
        .help("processing params serialized to json");

    program.add_argument("--postprocess")
        .help("postprocessing params serialized to json");

    program.add_argument("-o", "--output")
        .help("output file");

    try {
        program.parse_args(argc, argv);
    }
    catch (const std::exception& err) {
        std::cerr << err.what() << std::endl;
        std::cerr << program;
        return 1;
    }

    auto path = program.get<std::string>("path");

    auto params = get_process_default();
    if (program.present("--process")) {
        auto ok = false;
        auto input = program.get<std::string>("process");
        params = parse_process(&input, &ok);
        if (!ok) {
            std::cerr << "Failed to parse process parameters" << std::endl;
            return 1;
        }
    }

    PostProcessParams *postprocess = nullptr;
    PostProcessParams postprocess_val;
    if (program.present("--postprocess")) {
        auto postprocess_input = program.get<std::string>("--postprocess");
        auto ok = false;
        postprocess_val = parse_postprocess(&postprocess_input, &ok);
        postprocess = &postprocess_val;
        if (!ok) {
            std::cerr << "Failed to parse postprocess parameters" << std::endl;
            return 1;
        }
    }

    TString output;
    if (program.present("--output")) {
        output = TString(program.get<std::string>("--output"));
    } else {
        output = TString::Format("%s.root", std::filesystem::path(path).filename().string().c_str());
    }

    auto file = TFile::Open(
        output,
        "RECREATE"
    );

    auto meta = TString(get_meta(path));
    file->WriteObject(&meta, "meta");

    auto tree = process(
        path,
        &params,
        postprocess
    );

    file->Write();
    file->Close();

    return 0;
}
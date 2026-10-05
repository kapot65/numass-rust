#include <iostream>
#include <filesystem>

#include <TFile.h>
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

    // Параметры обработки. Пустая строка = параметры по умолчанию.
    std::string process_json;
    if (program.present("--process")) {
        process_json = program.get<std::string>("--process");
        if (!validate_process(&process_json)) {
            std::cerr << "Failed to parse process parameters" << std::endl;
            return 1;
        }
    }

    // Параметры пост-обработки. nullptr = пост-обработка отключена.
    std::string postprocess_json;
    bool has_postprocess = false;
    if (program.present("--postprocess")) {
        postprocess_json = program.get<std::string>("--postprocess");
        if (!validate_postprocess(&postprocess_json)) {
            std::cerr << "Failed to parse postprocess parameters" << std::endl;
            return 1;
        }
        has_postprocess = true;
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
        process_json,
        has_postprocess ? &postprocess_json : nullptr
    );

    file->Write();
    file->Close();

    return 0;
}

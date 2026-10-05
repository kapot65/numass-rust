#include <memory>

#include <iostream>
#include <fstream>

#include "TCanvas.h"
#include "TGraph.h"
#include "TMultiGraph.h"

#include "TString.h"

#include "TF1.h"
#include "TVector.h"

#include "ROOT/RCsvDS.hxx"

using namespace std;
using namespace ROOT;
using namespace ROOT::VecOps;

struct Channel {
    TGraph graph;
    TF1 *fitFunc;
    double left;
    double right;
    double peak;
};

struct Output {
    RDataFrame df;
    map<ushort, Channel> channels;
};

string ch_col(ushort ch_id) {
    std::ostringstream ss;
    ss << "ch " << ch_id + 1;
    return ss.str();
}  

// root 'fit.C+("kev", 0.3, 0.8)' for kev
// root 'fit.C+("raw", 1.0, 2.0)' for raw
map<int, Output*> fit(const char* folder, double l_range, double r_range) {
    map<int, Output*> out;
    
    ofstream out_file;
    out_file.open(TString::Format("./output/calibration_%s.csv", folder).Data());
    out_file << "voltage,1,2,3,4,6" << endl;

    int voltages[] = {12500, 13000, 13500, 14000, 14500, 15000, 15500, 16000, 16500, 17000};
    
    for (int i = 0; i < 8; i++) {

        out_file << voltages[i] << ",";

        auto voltage = voltages[i];

        auto df = ROOT::RDF::FromCSV(TString::Format("./%s/%i.csv", folder, voltage).Data());
        const RVec<Double_t> bins = df.Take<Double_t>("bin").GetValue();

        ushort ch_ids[] = {1, 2, 3, 4, 6}; // TODO: move into header
        auto channels = map<ushort, Channel>();

        for (auto ch_id : ch_ids) {

            auto graphResult = df.Graph("bin", TString::Format("ch %i", ch_id + 1).Data());
            auto graph = graphResult.GetValue();

            const RVec<Long64_t> ch_1 = df.Take<Long64_t>(ch_col(ch_id)).GetValue();
            auto max = bins[ArgMax(ch_1)];

            Double_t left = max - l_range;
            Double_t right = max + r_range; 

            graph.Fit("pol3", "", "", left, right);
            auto fitFunc = graph.GetFunction("pol3");
            auto peak = fitFunc->GetMaximumX(left, right);

            out_file << peak  << (ch_id == (ushort)6 ? "" : ",");

            channels[ch_id] = Channel {
                graph,
                fitFunc,
                left,
                right,
                peak
            };
        }

        out_file << endl;

        auto localOut = new Output {
            df, 
            channels
        };

        auto canvas = new TCanvas(TString::Format("%s %d", folder, i + 1), TString::Format(
            "pol3 fits for %s (left = %f, right = %f)", folder, l_range, r_range
        ));
        auto mg = new TMultiGraph();
        for (auto ch_id : ch_ids) {
            mg->Add(&localOut->channels[ch_id].graph, "lp");
        }
        out[voltage] = localOut;
        mg->SetTitle(TString::Format("%i", voltage));
        mg->Draw("a");
        canvas->Draw();
    }

    out_file.close();
    
    return out;
}
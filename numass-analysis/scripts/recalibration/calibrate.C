#include <memory>

#include <iostream>
#include <fstream>

#include "TString.h"
#include "TCanvas.h"

#include "Fit/FitData.h"

#include "TGraph.h"
#include "TLegend.h"
#include "TF1.h"
#include "TMultiGraph.h"
#include "TVector.h"

#include "ROOT/RCsvDS.hxx"

using namespace std;
using namespace ROOT;
using namespace ROOT::VecOps;

using json = nlohmann::json;


void calibrate() {

    auto raw_df = ROOT::RDF::FromCSV("./output/calibration_raw.csv");
    auto kev_df = ROOT::RDF::FromCSV("./output/calibration_kev.csv");

    auto ethalon = kev_df.Take<Double_t>(to_string(5)).GetValue();

    // auto coeffs = json::array();

    ofstream coeffsFile;
    coeffsFile.open ("./output/coeffs.json");


    coeffsFile << "[\n";
    for (int i = 1; i < 7; i++) {
        auto ch = raw_df.Take<Double_t>(to_string(i)).GetValue();

        TCanvas *c1 = new TCanvas(TString::Format("Channel %d", i));

        auto gr = new TGraph(11, ch.data(), ethalon.data());
        gr->SetMarkerStyle(2);

        gr->SetTitle(TString::Format("Channel %d", i));

        gr->Fit("pol1").Get();
        auto fitFunc = gr->GetFunction("pol1");

        auto p1 = fitFunc->GetParameter(1);
        auto p0 = fitFunc->GetParameter(0);

        auto l = new TLegend();
        l->AddEntry(fitFunc, TString::Format("y = %.4f * x + %.4f", p1, p0));

        {
            coeffsFile << "    [" << p1 << ", " << p0 << "]";
            if (i == 6) {
                coeffsFile << "\n";
            } else {
                coeffsFile << ",\n";
            }
        }
        
        c1->cd(i + 1);
        gr->Draw();
        l->Draw();

        c1->Draw();
    }

    coeffsFile << "]";
    coeffsFile.close();
}
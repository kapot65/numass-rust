// cargo run --release --bin tqdc-deadtime > scripts/deltas.tsv

#include <TTree.h>
#include <TF1.h>


auto fit_deltas() {
    auto tree = new TTree("t", "T");
    tree->ReadFile("11kev-deltas-all.tsv", "", '\t');

    tree->Draw("counts:delta", "", "");

    // (new TF1("fit","exp(7.73812 + -1.99708e-05 * x)", 0, 320000))->Draw("SAME");



    // graph.Fit("pol3", "", "", left, right);
    // auto fitFunc = graph.GetFunction("pol3");
    
}
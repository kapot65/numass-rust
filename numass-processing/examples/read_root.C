#include <ROOT/RDataFrame.hxx>
#include <ROOT/RNTupleReader.hxx>
#include <iostream>
#include <vector>

// root -b -q 'examples/read_root.C("file.root")'
void read_root(const char *path, int n = 5) {
   auto reader = ROOT::RNTupleReader::Open("T", path);
   std::vector<std::string> names;
   for (const auto &field : reader->GetDescriptor().GetTopLevelFields()) {
      std::cout << field.GetFieldName() << ": " << field.GetTypeName() << '\n';
      names.push_back(field.GetFieldName());
   }
   std::cout << "entries " << reader->GetNEntries() << '\n';
   ROOT::RDataFrame("T", path).Display(names, n)->Print();
}

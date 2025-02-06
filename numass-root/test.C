#include "bindings.h"
void test() {

    auto h = H();
    h.tag = H::Tag::H_Bar;
    h.bar.x = 10;
    h.bar.y = 20;

    eat_enum(h);
}
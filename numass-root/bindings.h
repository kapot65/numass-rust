#ifndef processing_h
#define processing_h

/* Generated with cbindgen:0.28.0 */

#include <cstdarg>
#include <cstddef>
#include <cstdint>
#include <cstdlib>
#include <ostream>
#include <new>
#include <string>
#include <memory>


/// Размер блока, который будет проверяться на наличие проблем (в нс)
static const uint64_t CHECK_BIN_SIZE = 10000000;

/// Порог по HV для проверки точки (точки с HV выше не будут проверяться)
static const float CHECK_HV_THRESHOLD = 16e3;

/// Размер блока, который будет вырезан, если в нем обнаружены проблемы (в нс)
static const uint64_t CUTOFF_BIN_SIZE = 1000000000;

enum class SkipOption {
  None,
  Bad,
  Good,
};

struct PointState;

struct HWResetParams {
  size_t window;
  int16_t treshold;
  size_t size;
};

/// Built-in algorithms params for processing the data.
struct Algorithm {
  enum class Tag {
    Max,
    Likhovid,
    FirstPeak,
    Trapezoid,
    LongDiff,
  };

  struct Likhovid_Body {
    size_t left;
    size_t right;
  };

  struct FirstPeak_Body {
    int16_t threshold;
    size_t left;
  };

  struct Trapezoid_Body {
    size_t left;
    size_t center;
    size_t right;
    int16_t treshold;
    size_t min_length;
    SkipOption skip;
    HWResetParams reset_detection;
  };

  struct LongDiff_Body {
    HWResetParams reset_detection;
  };

  Tag tag;
  union {
    Likhovid_Body likhovid;
    FirstPeak_Body first_peak;
    Trapezoid_Body trapezoid;
    LongDiff_Body long_diff;
  };
};

struct H {
  enum class Tag {
    H_Foo,
    H_Bar,
    H_Baz,
  };

  struct H_Foo_Body {
    int16_t _0;
  };

  struct H_Bar_Body {
    uint8_t x;
    int16_t y;
  };

  Tag tag;
  union {
    H_Foo_Body foo;
    H_Bar_Body bar;
  };
};












extern "C" {

void eat_algorithm(Algorithm algo);

void eat_callback(void (*callback)(const char*));

/// Необходимо использовать std::move !!!
/// Или не удалять объект на время выполнения функции
void eat_cxx_string(const std::string *str);

void eat_enum(H h);

void eat_string(const char *s);

/// возвращает [TRAPEZOID_DEFAULT] в С++ виде
Algorithm get_trapeziod_default();

void process_point(const std::string *path, const Algorithm *algo, void (*fill_fn)(uint64_t,
                                                                                   uint8_t,
                                                                                   float));

void say_hello();

}  // extern "C"

#endif  // processing_h

#pragma once

#include <cstdint>

// The statistics live in DXMT's core (Rust, src/dxmt/src/stats.rs); these are its C entry points, which the
// forwards below call. g_stats_on stays here and reaches the core by value at load (d3d12_stats.cpp).
extern "C" {
/* dxmt-abi begin */
void dxmt_stats_init(bool on);
void dxmt_stats_exit(void);
unsigned dxmt_stat_id(const char *name);
void dxmt_stat_count(unsigned id, uint64_t n);
uint64_t dxmt_stat_begin(void);
void dxmt_stat_end(unsigned id, uint64_t t0);
void dxmt_stats_frame(void);
unsigned dxmt_stats_next_pass(void);
/* dxmt-abi end */
}

namespace dxmt {

// MacNeutron: DXMT_STATS=1 (with DXMT_DXIL_DUMP=<folder>) times every D3D12 call per thread and appends a report to
// <folder>/stats.txt every 5 seconds of presents, and once at exit: calls and microseconds per frame of each call,
// the time each thread spends in D3D12 (outermost calls only), counters such as encoder boundaries with no barrier,
// and Metal encoders labelled "pass-<n> <kind>" (n counts as F9's passes.txt does) for Instruments. Off: one branch.
#ifdef DXMT_PERF_BUILD // MacNeutron's performance build: no stats, every call site's branch compiled out
constexpr bool g_stats_on = false;
inline unsigned StatId(const char *) { return 0; }
inline void StatCount(unsigned, uint64_t = 1) {}
inline void StatsFrame() {}
inline unsigned StatsNextPass() { return 0; }
inline uint64_t StatBegin() { return 0; }
inline void StatEnd(unsigned, uint64_t) {}
#else
extern bool g_stats_on;

inline unsigned StatId(const char *name) { return dxmt_stat_id(name); } // registers a copy; call sites keep the id
inline void StatCount(unsigned id, uint64_t n = 1) { dxmt_stat_count(id, n); } // a counter: no time
inline void StatsFrame() { dxmt_stats_frame(); }                                // at each present
inline unsigned StatsNextPass() { return dxmt_stats_next_pass(); } // numbers encoders from 0 at each present
inline uint64_t StatBegin() { return dxmt_stat_begin(); }
inline void StatEnd(unsigned id, uint64_t t0) { dxmt_stat_end(id, t0); }
#endif

struct StatScope {
  unsigned id;
  uint64_t t0;
  StatScope(unsigned id) : id(id), t0(g_stats_on ? StatBegin() : 0) {}
  ~StatScope() {
    if (g_stats_on)
      StatEnd(id, t0);
  }
};

} // namespace dxmt

#ifdef DXMT_PERF_BUILD
#define DXMT_STAT_SCOPE(name) ((void)0)
#define DXMT_STAT_COUNT(name, n) ((void)0)
#else
#define DXMT_STAT_SCOPE(name)                                                                                          \
  static const unsigned dxmt_stat_id_ = ::dxmt::StatId(name);                                                          \
  ::dxmt::StatScope dxmt_stat_scope_(dxmt_stat_id_)
#define DXMT_STAT_COUNT(name, n)                                                                                       \
  do {                                                                                                                 \
    if (::dxmt::g_stats_on) {                                                                                          \
      static const unsigned dxmt_stat_id_ = ::dxmt::StatId(name);                                                      \
      ::dxmt::StatCount(dxmt_stat_id_, n);                                                                             \
    }                                                                                                                  \
  } while (0)
#endif

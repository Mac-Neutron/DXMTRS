#include "d3d12_stats.hpp"
#include "util_env.hpp"

#ifndef DXMT_PERF_BUILD // MacNeutron's performance build has no stats (d3d12_stats.hpp)
namespace dxmt {

bool g_stats_on = env::getEnvVar("DXMT_STATS") == "1";

namespace {

// Starts the core's clock at load (it sets the capture folder up first), and reports once more at exit.
struct Lifetime {
  Lifetime() { dxmt_stats_init(g_stats_on); }
  ~Lifetime() { dxmt_stats_exit(); }
} g_lifetime;

} // namespace

} // namespace dxmt
#endif

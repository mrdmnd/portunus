#include <chrono>

namespace simulator {
namespace core {
// An Aura is a (usually) timed buff or debuff on an actor. It keeps track of
// how long it has until expiry, in milliseconds. If constructed with
// max_duration == std::chrono::milliseconds::zero(), this is an "infinite"
// duration aura. If constructed with max_stacks = 0, this is an "infinitely"
// stackable aura.

using SpellId = int;

class Aura {
 public:
  explicit Aura(SpellId spell_id,
                std::chrono::milliseconds max_duration,
                int max_stacks) :
    spell_id_(spell_id),
    max_duration_(max_duration),
    max_stacks_(max_stacks) {}

  // Explicitly disallow copy+move assign/construct.
  Aura(const Aura& other) = delete;
  Aura(const Aura&& other) = delete;
  Aura& operator=(const Aura& other) = delete;
  Aura& operator=(const Aura&& other) = delete;

  inline bool IsInfiniteDuration() const {
    return max_duration_ == std::chrono::milliseconds::zero();
  }

  inline std::chrono::milliseconds Remaining() const { return cur_duration_; }

  inline int Stacks() const { return cur_stacks_; }

 private:
  const SpellId spell_id_;
  std::chrono::milliseconds cur_duration_;
  const std::chrono::milliseconds max_duration_;
  int cur_stacks_;
  const int max_stacks_;
};
}  // namespace core
}  // namespace simulator

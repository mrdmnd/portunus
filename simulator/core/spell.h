#pragma once

namespace simulator {
namespace core {
// This classs provides a means for defining player spells.
// A spell has a static component describing properties of "all such" spells,
// and a non-static component describing properties pertinent to this
// instantiation of the spell.

// This is the static data component of a spell.
class SpellData {
  // See fields in
  // https://github.com/simulationcraft/simc/blob/f5f1a6d36bcf58c0a41bbd1e6405b473e77cb26b/engine/dbc/dbc.hpp#L807
};

// This is the instantiation data.
class SpellInstance {
 public:
  virtual bool Execute() const = 0;
  explicit SpellInstance(const SpellData& data, const Actor* source,
                         const Actor* target) :
    data_(data),
    source_(source),
    target_(target){};

 protected:
  // The spell data.
  const SpellData data_;

  // Instances of spells have source actors and target actors.
  const Actor* source_;
  const Actor* target_;
};
}  // namespace core
}  // namespace simulator

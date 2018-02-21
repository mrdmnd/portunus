#pragma once

#include "simulate/action.h"

// The Policy interface is intended to have two concrete subclasses:
// "Explicit" policy, which *literally* provides code to compute an action from
// a simulator state, and "ML" policy, which applies a tensor flow model to a
// simulator state's vector representation to generate a probabalistic action
// distribution that we sample from.
class Policy {
public:
  virtual Action ComputeAction() const = 0;
};

class ExplicitPolicy : public Policy {
public:
  Action ComputeAction() const { return Action("Hello, policygen!"); }
};

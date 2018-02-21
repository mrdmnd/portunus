#pragma once
#include <string>

class Action {
  Action(const std::string &name) : name_(name){};

private:
  std::string name_;
};

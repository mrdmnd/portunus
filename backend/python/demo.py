class State:
    def __init__(self, energy, chi):
        self.energy = energy
        self.chi = chi
        self.last_global = None


class TigerPalm:
    def usable(self, state):
        return state.energy >= 50

    def execute(self, state):
        state.chi = min(5, state.chi+2)
        state.energy -= 50
        state.last_global = "TigerPalm"
        return 1.0

class BlackoutKick:
    def usable(self, state):
        return state.chi >= 1

    def execute(self, state):
        state.chi -= 1
        state.last_global = "BlackoutKick"
        return 4.0

class RisingSunKick:
    def usable(self, state):
        return state.chi >= 2

    def execute(self, state):
        state.chi -= 2
        state.last_global = "RisingSunKick"
        return 10.0

def Policy(state):
    if 

s = State(100, 0)



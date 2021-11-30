class State:
    def __init__(self, gcd, cp, energy, opportunity_buff, target_hp):
        self.gcd = gcd
        self.cp = cp
        self.energy = energy
        self.opportunity_buff = opportunity_buff
        self.target_hp = target_hp

    def __repr__(self):
        return "State({0} gcd={1} cp={2} energy={3} opportunity_buff={4} target_hp={5}".format(hex(id(self)), self.gcd, self.cp, self.energy, self.opportunity_buff, self.target_hp)

    def IsTerminal(self):
        return self.target_hp < 0
 
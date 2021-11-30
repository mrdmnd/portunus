"""
A minimal implementation of Monte Carlo tree search (MCTS) in Python 3
Luke Harold Miles, July 2019, Public Domain Dedication
See also https://en.wikipedia.org/wiki/Monte_Carlo_tree_search
https://gist.github.com/qpwo/c538c6f73727e254fdc7fab81024f6e1

Modified by mrdmnd for my purposes Nov 2020
"""
from abc import ABC, abstractmethod
from collections import defaultdict
import math
import random


class MCTS:
    "Monte Carlo tree searcher. First rollout the tree then choose a move."

    def __init__(self, exploration_weight=math.sqrt(2)):
        self.Q = defaultdict(float)  # total reward of each node
        self.N = defaultdict(int)  # total visit count for each node
        self.children = dict()  # children of each node
        self.exploration_weight = exploration_weight

    def Choose(self, node):
        "Choose the best successor of node. (Choose a move in the game)"
        if node.IsTerminal():
            raise RuntimeError(f"choose called on terminal node {node}")

        if node not in self.children:
            return random.choice(node.FindChildren())

        def score(n):
            return float("-inf") if self.N[n] == 0 else self.Q[n] / self.N[n]

        return max(self.children[node], key=score)

    def DoRollout(self, node):
        "Make the tree one layer better. (Train for one iteration.)"
        path = self._select(node)
        leaf = path[-1]
        self._expand(leaf)
        reward = self._simulate(leaf)
        self._backpropagate(path, reward)

    def _select(self, node):
        "Find an unexplored descendent of `node`"
        path = []
        while True:
            path.append(node)
            if node not in self.children or not self.children[node]:
                # node is either unexplored or terminal
                return path
            unexplored = self.children[node] - self.children.keys()
            if unexplored:
                n = unexplored.pop()
                path.append(n)
                return path
            node = self._uct_select(node)  # descend a layer deeper

    def _expand(self, node):
        "Update the `children` dict with the children of `node`"
        if node in self.children:
            return  # already expanded
        self.children[node] = node.FindChildren()

    def _simulate(self, node):
        "Returns the reward for a random simulation (to completion) of `node`"
        while True:
            if node.IsTerminal():
                return node.Reward()
            node = random.choice(node.FindChildren())

    def _backpropagate(self, path, reward):
        "Send the reward back up to the ancestors of the leaf"
        for node in reversed(path):
            self.N[node] += 1
            self.Q[node] += reward

    def _uct_select(self, node):
        "Select a child of node, balancing exploration & exploitation"

        # All children of node should already be expanded:
        assert all(n in self.children for n in self.children[node])

        def uct(n):
            "Upper confidence bound for trees"
            return self.Q[n] / self.N[n] + self.exploration_weight * math.sqrt(math.log(self.N[node]) / self.N[n])

        return max(self.children[node], key=uct)


class Node(ABC):
    """
    A representation of a single board state.
    MCTS works by constructing a tree of these Nodes.
    Could be e.g. a chess or checkers board state.
    """

    @abstractmethod
    def FindChildren(self):
        "All possible successors of this board state"
        return set()

    @abstractmethod
    def IsTerminal(self):
        "Returns True if the node has no children"
        return True

    @abstractmethod
    def Reward(self):
        "Assumes `self` is terminal node. 1=win, 0=loss, .5=tie, etc"
        return 0

    @abstractmethod
    def __hash__(self):
        "Nodes must be hashable"
        return 123456789

    @abstractmethod
    def __eq__(node1, node2):
        "Nodes must be comparable"
        return True

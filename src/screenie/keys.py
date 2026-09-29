"""Key names and small input helpers shared by every mode.

GTK key names come in variants (+ is "plus", "equal" unshifted, "KP_Add" on
the keypad); the App also feeds the mouse in as keys (wheel = + / -, right
click = Enter, middle click = Esc), so modes only ever test these sets.
"""

from .geometry import VEC

INC = frozenset({"plus", "equal", "KP_Add"})
DEC = frozenset({"minus", "KP_Subtract"})
AMOUNT = INC | DEC
CONFIRM = frozenset({"Return", "KP_Enter"})
BACK = "Escape"

# w b } { : the next edge that way; 0 $ g G : the far edge that way
EDGE_JUMPS = {"w": "R", "b": "L", "braceright": "B", "braceleft": "T"}
FAR_EDGES = {"0": "L", "dollar": "R", "g": "T", "G": "B"}

LOUPE = frozenset({"z", "Z"})


def held_vector(dirs):
    """Held directions as one (dx, dy): k+l is diagonal, h+l cancel."""
    return sum(VEC[d][0] for d in dirs), sum(VEC[d][1] for d in dirs)


def hex_colour(rgb):
    return "#%02x%02x%02x" % tuple(rgb)

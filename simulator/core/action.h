#pragma once

// An *ACTION* causes one or more *EFFECTS* (see effect.h).
// Effects cause "pure mutations" on simulation state.
// A single ACTION may have multiple effects.
// Actions may have triggers that automatically cause them to fire
// (e.g. Second Shuriken, which fires 30% of the time Shuriken Storm goes off)

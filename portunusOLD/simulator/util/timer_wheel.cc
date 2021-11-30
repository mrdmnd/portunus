#include <algorithm>

#include "glog/logging.h"

#include "simulator/util/timer_wheel.h"
namespace simulator {
namespace util {
TimerEventInterface* TimerWheelSlot::PopEvent() {
  auto event = events_;
  events_ = event->next_;
  if (events_) {
    events_->prev_ = NULL;
  }
  event->next_ = NULL;
  event->slot_ = NULL;
  return event;
}

TimerWheel::TimerWheel(Tick now) {
  for (int i = 0; i < NUM_LEVELS; ++i) {
    now_[i] = now >> (WIDTH_BITS * i);
  }
  ticks_pending_ = 0;
}

void TimerEventInterface::Relink(TimerWheelSlot* new_slot) {
  if (new_slot == slot_) {
    return;
  }

  // Unlink from old location.
  if (slot_) {
    auto prev = prev_;
    auto next = next_;
    if (next) {
      next->prev_ = prev;
    }
    if (prev) {
      prev->next_ = next;
    } else {
      // Must be at head of slot. Move the next item to the head.
      slot_->events_ = next;
    }
  }

  // Insert in new slot.
  if (new_slot) {
    auto old = new_slot->events_;
    next_ = old;
    if (old) {
      old->prev_ = this;
    }
    new_slot->events_ = this;
  } else {
    next_ = NULL;
  }
  prev_ = NULL;
  slot_ = new_slot;
}

void TimerEventInterface::Cancel() {
  // It's ok to cancel a event that's not scheduled.
  if (!slot_) {
    return;
  }
  Relink(NULL);
}

// This handles the actual work of executing event callbacks and
// recursing to the outer wheels.
bool TimerWheel::ProcessCurrentSlot(Tick now, size_t max_events, int level) {
  size_t slot_index = now & MASK;
  auto slot = &slots_[level][slot_index];
  if (slot_index == 0 && level < MAX_LEVEL) {
    if (!Advance(1, max_events, level + 1)) {
      return false;
    }
  }
  while (slot->Events()) {
    auto event = slot->PopEvent();
    if (level > 0) {
      DCHECK((now_[0] & MASK) == 0) << "now_[0] & MASK != 0";
      if (now_[0] >= event->ScheduledAt()) {
        event->Execute();
        if (!--max_events) {
          return false;
        }
      } else {
        // There's a case to be made that promotion should
        // also count as work done. And that would simplify
        // this code since the max_events manipulation could
        // move to the top of the loop. But it's an order of
        // magnitude more expensive to execute a typical
        // callback, and promotions will naturally clump while
        // events triggering won't.
        Schedule(event, event->ScheduledAt() - now_[0]);
      }
    } else {
      event->Execute();
      if (!--max_events) {
        return false;
      }
    }
  }
  return true;
}

bool TimerWheel::Advance(Tick delta, size_t max_events, int level) {
  if (ticks_pending_) {
    if (level == 0) {
      // Continue collecting a backlog of ticks to process if
      // we're called with non-zero deltas.
      ticks_pending_ += delta;
    }
    // We only partially processed the last tick. Process the
    // current slot, rather incrementing like advance() normally
    // does.
    Tick now = now_[level];
    if (!ProcessCurrentSlot(now, max_events, level)) {
      // Outer layers are still not done, propagate that information
      // back up.
      return false;
    }
    if (level == 0) {
      // The core wheel has been fully processed. We can now close
      // down the partial tick and pretend that we've just been
      // called with a delta containing both the new and original
      // amounts.
      delta = (ticks_pending_ - 1);
      ticks_pending_ = 0;
    } else {
      return true;
    }
  } else {
    // Zero deltas are only ok when in the middle of a partially
    // processed tick.
    DCHECK(delta > 0) << "Delta <= 0";
  }

  while (delta--) {
    Tick now = ++now_[level];
    if (!ProcessCurrentSlot(now, max_events, level)) {
      ticks_pending_ = (delta + 1);
      return false;
    }
  }
  return true;
}

void TimerWheel::Schedule(TimerEventInterface* event, Tick delta) {
  DCHECK(delta > 0) << "Delta <= 0";
  event->Reschedule(now_[0] + delta);

  int level = 0;
  while (delta >= NUM_SLOTS) {
    delta = (delta + (now_[level] & MASK)) >> WIDTH_BITS;
    ++level;
  }

  size_t slot_index = (now_[level] + delta) & MASK;
  auto slot = &slots_[level][slot_index];
  event->Relink(slot);
}

Tick TimerWheel::TicksUntilNextEvent(Tick max, int level) const {
  if (ticks_pending_) {
    return 0;
  }

  // The actual current time (not the bitshifted time)
  Tick now = now_[0];

  // Smallest tick (relative to now) we've found.
  Tick min = max;
  for (int i = 0; i < NUM_SLOTS; ++i) {
    // Note: Unlike the uses of "now", slot index calculations really
    // need to use now_.
    auto slot_index = (now_[level] + 1 + i) & MASK;
    // We've reached slot 0. In normal scheduling this would
    // mean advancing the next wheel and promoting or executing
    // those events.  So we need to look in that slot too
    // before proceeding with the rest of this wheel. But we
    // can't just accept those results outright, we need to
    // check the best result there against the next slot on
    // this wheel.
    if (slot_index == 0 && level < MAX_LEVEL) {
      // Exception: If we're in the core wheel, and slot 0 is
      // not empty, there's no point in looking in the outer wheel.
      // It's guaranteed that the events actually in slot 0 will be
      // executed no later than anything in the outer wheel.
      if (level > 0 || !slots_[level][slot_index].Events()) {
        auto up_slot_index = (now_[level + 1] + 1) & MASK;
        const auto& slot = slots_[level + 1][up_slot_index];
        for (auto event = slot.Events(); event != NULL; event = event->next_) {
          min = std::min(min, event->ScheduledAt() - now);
        }
      }
    }
    bool found = false;
    const auto& slot = slots_[level][slot_index];
    for (auto event = slot.Events(); event != NULL; event = event->next_) {
      min = std::min(min, event->ScheduledAt() - now);
      // In the core wheel all the events in a slot are guaranteed to
      // run at the same time, so it's enough to just look at the first
      // one.
      if (level == 0) {
        return min;
      } else {
        found = true;
      }
    }
    if (found) {
      return min;
    }
  }

  // Nothing found on this wheel, try the next one (unless the wheel can't
  // possibly contain an event scheduled earlier than "max").
  if (level < MAX_LEVEL && (max >> (WIDTH_BITS * level + 1)) > 0) {
    return TicksUntilNextEvent(max, level + 1);
  }
  return max;
}
}  // namespace util
}  // namespace simulator

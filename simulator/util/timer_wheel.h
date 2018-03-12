#pragma once

#include <cstddef>
#include <cstdint>
#include <functional>
#include <limits>

// https://blog.acolyer.org/2015/11/23/hashed-and-hierarchical-timing-wheels/
// https://www.snellman.net/blog/archive/2016-07-27-ratas-hierarchical-timer-wheel/
//
// Code adapted from Ratas implementation, which is MIT Licensed.
// https://github.com/jsnell/ratas/blob/master/src/timer-wheel.h
//
// It has been modified to remove the MemberTimerEvent and schedule_in_range,
// neither of which we need. Additionally, we support callbacks that aren't
// just std::function<void()>.
//
// The exact implementation strategy is a hierarchical timer
// wheel. A timer wheel is effectively a ring buffer of linked lists
// of events, and a pointer to the ring buffer. As the time advances,
// the pointer moves forward, and any events in the ring buffer slots
// that the pointer passed will get executed.
//
// A hierarchical timer wheel layers multiple timer wheels running at
// different resolutions on top of each other. When an event is
// scheduled so far in the future that it does not fit the innermost
// (core) wheel, it instead gets scheduled on one of the outer
// wheels. On each rotation of the inner wheel, one slot's worth of
// events are promoted from the second wheel to the core. On each
// rotation of the second wheel, one slot's worth of events is
// promoted from the third wheel to the second, and so on.
//
// The basic usage is to create a single TimerWheel object and
// multiple TimerEvents The events are scheduled for execution using
// TimerWheel::Schedule() or unscheduled using the event's Cancel() method.
//
// It is important to note that the TimerWheel does *NOT* own its TimerEvents,
// and should not be used to manage their lifetime.
namespace simulator {
namespace util {

using Tick = uint64_t;

class TimerWheelSlot;
class TimerWheel;

// A timer task is a struct holding a std::function and its argument.
// Essentially, we construct a
template <typename CBType, typename ArgType>
class TimerTask {
 public:
  TimerTask<CBType, ArgType>(CBType callback, ArgType arg) :
    callback(callback),
    arg(arg){};

  CBType callback;
  ArgType arg;
};

// An abstract class representing an event that can be scheduled to
// happen at some later time.
class TimerEventInterface {
 public:
  TimerEventInterface() {}
  TimerEventInterface(const TimerEventInterface& other) = delete;
  TimerEventInterface(const TimerEventInterface&& other) = delete;
  TimerEventInterface& operator=(const TimerEventInterface& other) = delete;
  TimerEventInterface&& operator=(const TimerEventInterface&& other) = delete;

  // TimerEvents are automatically canceled on destruction.
  virtual ~TimerEventInterface() { Cancel(); }

  // Unschedule this event. It's safe to cancel an event that is inactive.
  void Cancel();

  // Return true iff the event is currently scheduled for execution.
  inline bool Active() const { return slot_ != nullptr; }

  // Return the absolute tick this event is scheduled to be executed on.
  inline Tick ScheduledAt() const { return scheduled_at_; }

  // Reschedule event to tick `ts`.
  inline void Reschedule(Tick ts) { scheduled_at_ = ts; }

 private:
  friend TimerWheelSlot;
  friend TimerWheel;

  // When should this event fire?
  Tick scheduled_at_;

  // Parent slot this event is currently in (NULL if not currently scheduled).
  TimerWheelSlot* slot_ = nullptr;

  // The events are linked together in the slot using an internal doubly-linked
  // list; this iterator does double duty as the linked list for this event.
  TimerEventInterface* next_ = nullptr;
  TimerEventInterface* prev_ = nullptr;

  // Implemented in subclasses. Executes the event callback.
  virtual void Execute() = 0;

  // Move the event to another slot.
  // (It's safe for either the current or new slot to be NULL).
  void Relink(TimerWheelSlot* slot);
};

template <class CBType, class ArgType>
class TimerEvent : public TimerEventInterface {
 public:
  explicit TimerEvent<CBType, ArgType>(const TimerTask<CBType, ArgType>& task) :
    task_(task) {}
  TimerEvent<CBType, ArgType>(const TimerEvent<CBType, ArgType>& other) =
      delete;
  TimerEvent<CBType, ArgType>(const TimerEvent<CBType, ArgType>&& other) =
      delete;
  TimerEvent<CBType, ArgType>& operator=(
      const TimerEvent<CBType, ArgType>& other) = delete;
  TimerEvent<CBType, ArgType>& operator=(
      const TimerEvent<CBType, ArgType>&& other) = delete;

  // Actually, you know, do the thing you're supposed to do.
  void Execute() override { std::invoke(task_.callback, task_.arg); }

 private:
  TimerTask<CBType, ArgType> task_;
};

// Just an implementation detail of TimerWheel to hold the linked list.
class TimerWheelSlot {
 public:
  TimerWheelSlot() {}
  TimerWheelSlot(const TimerWheelSlot& other) = delete;
  TimerWheelSlot(const TimerWheelSlot&& other) = delete;
  TimerWheelSlot& operator=(const TimerWheelSlot& other) = delete;
  TimerWheelSlot& operator=(const TimerWheelSlot&& other) = delete;

 private:
  friend TimerEventInterface;
  friend TimerWheel;

  // Doubly linked list of events.
  TimerEventInterface* events_ = nullptr;

  // Return pointer to first event queued in this slot.
  inline const TimerEventInterface* Events() const { return events_; }

  // De-queue the first event from the slot, and return it.
  TimerEventInterface* PopEvent();
};

// A TimerWheel is the entity that TimerEvents can be scheduled for execution
// with Schedule(), and will eventually be executed once the time advances far
// enough with the Advance() method.
class TimerWheel {
 public:
  explicit TimerWheel(Tick now);
  TimerWheel() : TimerWheel(0){};
  TimerWheel(const TimerWheel& other) = delete;
  TimerWheel(const TimerWheel&& other) = delete;
  TimerWheel& operator=(const TimerWheel& other) = delete;
  TimerWheel& operator=(const TimerWheel&& other) = delete;

  // Advance the TimerWheel by the specified number of ticks, and execute
  // any events scheduled for execution at or before that time. The
  // number of events executed can be restricted using the max_execute
  // parameter. If that limit is reached, the function will return false,
  // and the excess events will be processed on a subsequent call.
  //
  // - It is safe to cancel or schedule events from within event callbacks.
  // - During the execution of the callback the observable event tick will
  //   be the tick it was scheduled to run on; not the tick the clock will
  //   be advanced to.
  // - Events will happen in order; all events scheduled for tick X will
  //   be executed before any event scheduled for tick X+1.
  //
  // Delta should be non-0. The only exception is if the previous
  // call to Advance() returned false.
  //
  // Advance() should not be called from an event callback.
  bool Advance(Tick delta,
               size_t max_execute = std::numeric_limits<size_t>::max(),
               int level = 0);

  // Schedule the event to be executed delta ticks from the current time.
  // The delta must be non-0.
  void Schedule(TimerEventInterface* event, Tick delta);

  // Return the current tick value. Note that if the time increases
  // by multiple ticks during a single call to Advance(), during the
  // execution of the event callback Now() will return the tick that
  // the event was scheduled to run on.
  inline Tick Now() const { return now_[0]; }

  // Return the number of ticks remaining until the next event will get
  // executed. If the max parameter is passed, that will be the maximum
  // tick value that gets returned. The max parameter's value will also
  // be returned if no events have been scheduled.
  //
  // Will return 0 if the wheel still has unprocessed events from the
  // previous call to Advance().
  Tick TicksUntilNextEvent(Tick max = std::numeric_limits<Tick>::max(),
                           int level = 0) const;

 private:
  static const int WIDTH_BITS = 8;
  static const int NUM_LEVELS = (64 + WIDTH_BITS - 1) / WIDTH_BITS;
  static const int MAX_LEVEL = NUM_LEVELS - 1;
  static const int NUM_SLOTS = 1 << WIDTH_BITS;
  static const int MASK = (NUM_SLOTS - 1);

  // The current timestamp for this wheel. This will be right-shifted
  // such that each slot is separated by exactly one tick even on
  // the outermost wheels.
  Tick now_[NUM_LEVELS];

  // We've done a partial tick advance. This is how many ticks remain
  // unprocessed.
  Tick ticks_pending_;

  TimerWheelSlot slots_[NUM_LEVELS][NUM_SLOTS];  // by default, 8x256

  // This handles the actual work of executing event callbacks and
  // recursing to the outer wheels. This is a hot method!
  bool ProcessCurrentSlot(Tick now, size_t max_execute, int level);
};
}  // namespace util
}  // namespace simulator

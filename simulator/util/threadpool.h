#pragma once

#include <condition_variable>
#include <functional>
#include <future>
#include <mutex>
#include <queue>
#include <vector>

// Taken from https://github.com/progschj/ThreadPool/blob/master/ThreadPool.h
namespace simulator {
namespace util {
class ThreadPool {
 public:
  ThreadPool(size_t n_threads);
  ~ThreadPool();

  // Add a new item to the pool.
  template <class F, class... Args>
  auto Enqueue(F&& f, Args&&... args)
      -> std::future<typename std::result_of<F(Args...)>::type> {
    using return_type = typename std::result_of<F(Args...)>::type;

    auto task = std::make_shared<std::packaged_task<return_type()>>(
        std::bind(std::forward<F>(f), std::forward<Args>(args)...));

    std::future<return_type> res = task->get_future();
    {
      std::unique_lock<std::mutex> lock(queue_mutex_);

      // don't allow enqueueing after stopping the pool
      // if (stop_) throw std::runtime_error("enqueue on stopped ThreadPool");

      tasks_.emplace([task]() { (*task)(); });
    }
    condition_.notify_one();
    return res;
  }

  inline size_t NumThreads() { return workers_.size(); }

 private:
  std::vector<std::thread> workers_;
  std::queue<std::function<void()>> tasks_;
  std::mutex queue_mutex_;
  std::condition_variable condition_;
  bool stop_;
};
}  // namespace util
}  // namespace simulator

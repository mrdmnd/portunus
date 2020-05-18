pub struct Aggregator {
    n: i64,
    moment1: f64,
    moment2: f64,
}

impl Aggregator {
    pub fn new() -> Aggregator {
        Aggregator {
            n: 0,
            moment1: 0.0,
            moment2: 0.0,
        }
    }

    pub fn add_value(&mut self, value: f64) {
        self.n += 1;
        let delta1 = value - self.moment1;
        self.moment1 += delta1 / self.n as f64;
        let delta2 = value - self.moment1;
        self.moment2 += delta1 * delta2;
        return;
    }

    pub fn count(&self) -> i64 {
        return self.n;
    }

    pub fn mean(&self) -> f64 {
        return self.moment1;
    }

    pub fn variance(&self) -> f64 {
        return self.moment2 / (self.n - 1) as f64;
    }

    pub fn std_error(&self) -> f64 {
        return (self.moment2 / (self.n * (self.n - 1)) as f64).sqrt();
    }

    pub fn normalized_error(&self) -> f64 {
        return (self.moment2 / (self.n * (self.n - 1)) as f64).sqrt() / self.moment1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_one_value() {
        let mut agg = Aggregator::new();
        agg.add_value(1.0);
        assert_eq!(agg.count(), 1);
        assert_eq!(agg.mean(), 1.0);
        assert!(agg.variance().is_nan());
        assert!(agg.std_error().is_nan());
        assert!(agg.normalized_error().is_nan());
    }

    #[test]
    fn add_two_values() {
        let mut agg = Aggregator::new();
        agg.add_value(1.0);
        agg.add_value(2.0);
        assert_eq!(agg.count(), 2);
        assert_eq!(agg.mean(), 1.5);
        assert_eq!(agg.variance(), 0.5);
        assert_eq!(agg.std_error(), 0.5);
        assert_eq!(agg.normalized_error(), 1.0 / 3.0);
    }

    #[test]
    fn add_three_values() {
        let mut agg = Aggregator::new();
        agg.add_value(1.0);
        agg.add_value(2.0);
        agg.add_value(3.0);
        assert_eq!(agg.count(), 3);
        assert_eq!(agg.mean(), 2.0);
        assert_eq!(agg.variance(), 1.0);
        assert_eq!(agg.std_error(), (1.0 / 3.0 as f64).sqrt());
        assert_eq!(agg.normalized_error(), (1.0 / 3.0 as f64).sqrt() / 2.0);
    }
}

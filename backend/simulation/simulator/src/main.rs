mod online_statistics;

fn main() {
    let mut agg = online_statistics::Aggregator::new();
    agg.add_value(1.0);
    println!("Hello, world!");
}

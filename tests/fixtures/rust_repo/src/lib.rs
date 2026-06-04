pub fn add(a: i32, b: i32) -> i32 {
    a + b
}

pub fn factorial(n: u64) -> u64 {
    if n == 0 {
        1
    } else {
        n * factorial(n - 1)
    }
}

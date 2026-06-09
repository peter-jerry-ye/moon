fn get_platform() -> Int = "moonbit_v0" "get_platform"

///|
fn ms_since_epoch() -> Int64 = "moonbit_v0" "get_ms_since_epoch"

///|
fn sleep_ms(duration : Int) -> Int = "moonbit_v0" "sleep_ms"

///|
fn get_errno() -> Int = "moonbit_v0" "get_errno"

///|
fn make_tcp_socket(family : Int) -> Int = "moonbit_v0" "make_tcp_socket"

///|
fn assert_true(cond : Bool, message : String) -> Unit {
  if !cond {
    abort(message)
  }
}

///|
fn main {
  let platform = get_platform()
  assert_true(platform >= 0 && platform <= 2, "invalid platform")
  let before = ms_since_epoch()
  assert_true(sleep_ms(1) == 0, "sleep failed")
  let after = ms_since_epoch()
  assert_true(after >= before, "time moved backwards")
  assert_true(make_tcp_socket(0) == -1, "unsupported socket stub succeeded")
  assert_true(get_errno() != 0, "unsupported socket stub did not set errno")
  println("ok")
}

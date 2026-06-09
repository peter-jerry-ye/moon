fn get_platform() -> Int = "moonbit_v0" "get_platform"

///|
fn ms_since_epoch() -> Int64 = "moonbit_v0" "get_ms_since_epoch"

///|
fn sleep_ms(duration : Int) -> Int = "moonbit_v0" "sleep_ms"

///|
fn get_errno() -> Int = "moonbit_v0" "get_errno"

///|
fn get_tmp_path_len() -> Int = "moonbit_v0" "get_tmp_path_len"

///|
fn get_tmp_path(dst : Int, len : Int) -> Int = "moonbit_v0" "get_tmp_path"

///|
#borrow(bytes)
extern "wasm" fn bytes_to_ptr(bytes : Bytes) -> Int =
  #|(func (param i32) (result i32) local.get 0)

///|
fn errno_is_lock_violation(errno : Int) -> Bool = "moonbit_v0" "errno_is_lock_violation"

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
  let tmp_path_len = get_tmp_path_len()
  assert_true(tmp_path_len > 0, "tmp path size query failed")
  assert_true(get_tmp_path(0, 0) == -1, "tmp path fill accepted a missing buffer")
  let tmp_path = Bytes::make(tmp_path_len, b'\x00')
  let tmp_path_ptr = bytes_to_ptr(tmp_path)
  assert_true(get_tmp_path(tmp_path_ptr, tmp_path_len) == 0, "tmp path fill failed")
  assert_true(tmp_path[0] != b'\x00', "tmp path fill wrote an empty path")
  assert_true(!errno_is_lock_violation(0), "zero errno is a lock violation")
  assert_true(make_tcp_socket(0) == -1, "unsupported socket stub succeeded")
  assert_true(get_errno() != 0, "unsupported socket stub did not set errno")
  println("ok")
}

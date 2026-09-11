use crate::{batch::run_next_app, info, warn};

/// 执行 exit 系统调用
///
/// - code: 退出码
pub fn sys_exit(code: i32) -> ! {
    if code == 0 {
        info!("Program exited with code 0.");
    } else {
        warn!("Program exited with code {}.", code);
    }
    run_next_app()
}

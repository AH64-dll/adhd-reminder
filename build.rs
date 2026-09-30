fn main() {
    slint_build::compile_with_config(
        "ui/reminder.slint",
        slint_build::CompilerConfiguration::new().with_style("fluent".into()),
    )
    .expect("compile reminder UI");
}

include!("../winres.rs");

fn main() {
    let icon = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/icon.ico");
    describe("Dota2ModEnjoyer.exe", "Dota 2 Mod Enjoyer", Some(&icon));
}

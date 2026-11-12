pub const REPOSITORY: &str = "https://github.com/elangrr/Dota2ModEnjoyer";

pub const PAYPAL: &str = "https://paypal.me/elangrr";

pub struct Wallet {
    pub coin: &'static str,
    pub address: &'static str,
}

pub const WALLETS: &[Wallet] = &[
    Wallet { coin: "Solana (SOL), on the Solana network", address: "122Y7kRw7ELd4SpVG9LPCfVMtQgHiWDiSKakQom966RG" },
    Wallet { coin: "Ethereum (ETH), on the Ethereum network", address: "0xccf73af707f8bd2ee81dec77467b473705ef901e" },
];

pub fn takes_donations() -> bool {
    !PAYPAL.is_empty() || !WALLETS.is_empty()
}

fn may_open(address: &str) -> bool {
    address.strip_prefix("https://").is_some_and(|rest| !rest.is_empty() && !rest.contains(char::is_whitespace))
}

#[cfg(windows)]
pub fn open(address: &str) -> bool {
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    if !may_open(address) {
        return false;
    }
    let wide = |text: &str| text.encode_utf16().chain(Some(0)).collect::<Vec<u16>>();
    let (verb, page) = (wide("open"), wide(address));
    let answer = unsafe { ShellExecuteW(std::ptr::null_mut(), verb.as_ptr(), page.as_ptr(), std::ptr::null(), std::ptr::null(), SW_SHOWNORMAL) };
    answer as usize > 32
}

#[cfg(not(windows))]
pub fn open(_address: &str) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_web_pages_asked_for_safely_are_opened() {
        assert!(may_open(REPOSITORY));
        assert!(PAYPAL.is_empty() || may_open(PAYPAL));
        for other in ["http://example.org", "file:///C:/x.exe", "C:\\x.exe", "steam://run/570", "https://", "https://a b", ""] {
            assert!(!may_open(other), "{other}");
            assert!(!open(other), "{other}");
        }
    }

    #[test]
    fn what_is_offered_is_filled_in_whole() {
        for wallet in WALLETS {
            assert!(!wallet.coin.trim().is_empty() && !wallet.address.trim().is_empty());
            assert_eq!(wallet.address, wallet.address.trim(), "an address is copied as it is written");
        }
        assert_eq!(takes_donations(), !PAYPAL.is_empty() || !WALLETS.is_empty());

        let base58 = |letter: char| letter.is_ascii_alphanumeric() && !"0OIl".contains(letter);
        let solana = |address: &str| (32..=44).contains(&address.len()) && address.chars().all(base58);
        let ethereum = |address: &str| address.len() == 42 && address.strip_prefix("0x").is_some_and(|rest| rest.chars().all(|letter| letter.is_ascii_hexdigit()));
        for wallet in WALLETS {
            let shaped = if wallet.coin.starts_with("Solana") { solana(wallet.address) } else { ethereum(wallet.address) };
            assert!(shaped, "{}", wallet.coin);
        }
    }
}

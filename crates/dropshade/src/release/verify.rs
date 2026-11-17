//! Checking a release's files against its `SHA256SUMS`, which CI signs with the release key.
//! The signature's trusted comment names the version, so an older release's signed checksums
//! cannot stand in for a newer one's.

use std::collections::BTreeMap;

use minisign_verify::{PublicKey, Signature};
use semver::Version;
use sha2::{Digest, Sha256};

const RELEASE_KEY: &str = include_str!("../../../../packaging/minisign.pub");

/// The checksums of one release, after their signature was checked.
#[derive(Debug)]
pub struct Sums(BTreeMap<String, String>);

impl Sums {
    pub fn verified(sums: &str, signature: &str, version: &Version) -> Result<Self, String> {
        let key =
            PublicKey::decode(RELEASE_KEY).map_err(|error| format!("bad release key: {error}"))?;
        Self::verified_with(&key, sums, signature, version)
    }

    fn verified_with(
        key: &PublicKey,
        sums: &str,
        signature: &str,
        version: &Version,
    ) -> Result<Self, String> {
        let signature = Signature::decode(signature)
            .map_err(|error| format!("the release's signature is unreadable: {error}"))?;
        key.verify(sums.as_bytes(), &signature, false)
            .map_err(|_| "the release's checksums are not signed by the release key".to_owned())?;
        let expected = format!("dropshade {version}");
        if signature.trusted_comment() != expected {
            return Err(format!(
                "the release's signature is for {:?}, not {expected:?}",
                signature.trusted_comment()
            ));
        }
        Ok(Self(parse(sums)))
    }

    pub fn hash(&self, name: &str) -> Result<&str, String> {
        self.0
            .get(name)
            .map(String::as_str)
            .ok_or_else(|| format!("{name} is missing from the release's checksums"))
    }

    pub fn check(&self, name: &str, content: &[u8]) -> Result<(), String> {
        if sha256_hex(content) == self.hash(name)? {
            Ok(())
        } else {
            Err(format!("{name} does not match its checksum"))
        }
    }
}

/// `sha256sum` lines: the hash, a space, then a space (text) or `*` (binary) and the name.
fn parse(sums: &str) -> BTreeMap<String, String> {
    sums.lines()
        .filter_map(|line| {
            let (hash, name) = line.split_once(' ')?;
            let name = name.strip_prefix([' ', '*'])?;
            (hash.len() == 64 && hash.bytes().all(|byte| byte.is_ascii_hexdigit()))
                .then(|| (name.to_owned(), hash.to_ascii_lowercase()))
        })
        .collect()
}

fn sha256_hex(content: &[u8]) -> String {
    use std::fmt::Write;
    Sha256::digest(content)
        .iter()
        .fold(String::with_capacity(64), |mut hex, byte| {
            let _ = write!(hex, "{byte:02x}");
            hex
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_KEY: &str = "untrusted comment: minisign public key: 278E875902F60E13\n\
        RWQTDvYCWYeOJ8PwyKy6zAGzRF26R6w7/m7e6JxPplx2zM3jhatKayX0\n";
    const SUMS: &str = "\
aaaa000000000000000000000000000000000000000000000000000000000000  install.sh
bbbb000000000000000000000000000000000000000000000000000000000000  dropshade-1.2.3-x86_64-linux.tar.gz
";
    const SIGNATURE: &str = "untrusted comment: signature from rsign secret key\n\
        RUQTDvYCWYeOJ7G6JeZnTXtAEh5cQmh90J96+7rUJDOJUvezZsvOAvIx0JhTOA1IcbsHjZJhGaVbXYpfzMnjVeygBhxZkgif8Q4=\n\
        trusted comment: dropshade 1.2.3\n\
        a1lzafNhaX94esM56msqcDJ08Lu2xJ/LWRjRMmaR+UEUeBxSN9jRYF4Ibc2L2SOMQHbpgZyN28L5Ssf09C8wCw==\n";

    fn key() -> PublicKey {
        PublicKey::decode(TEST_KEY).expect("test key")
    }

    fn version(text: &str) -> Version {
        Version::parse(text).expect("version")
    }

    #[test]
    fn the_release_key_is_readable() {
        assert!(PublicKey::decode(RELEASE_KEY).is_ok());
    }

    #[test]
    fn signed_checksums_are_accepted() {
        let sums = Sums::verified_with(&key(), SUMS, SIGNATURE, &version("1.2.3")).expect("valid");
        assert_eq!(
            sums.hash("install.sh"),
            Ok("aaaa000000000000000000000000000000000000000000000000000000000000")
        );
        assert!(sums.hash("other.tar.gz").is_err());
    }

    #[test]
    fn changed_checksums_are_rejected() {
        let tampered = SUMS.replacen("aaaa", "cccc", 1);
        assert!(Sums::verified_with(&key(), &tampered, SIGNATURE, &version("1.2.3")).is_err());
    }

    #[test]
    fn another_key_is_rejected() {
        assert!(Sums::verified(SUMS, SIGNATURE, &version("1.2.3")).is_err());
    }

    #[test]
    fn checksums_signed_for_another_version_are_rejected() {
        let error = Sums::verified_with(&key(), SUMS, SIGNATURE, &version("1.2.4")).unwrap_err();
        assert!(error.contains("dropshade 1.2.3"), "{error}");
    }

    #[test]
    fn a_garbled_signature_is_rejected() {
        assert!(Sums::verified_with(&key(), SUMS, "nonsense", &version("1.2.3")).is_err());
    }

    #[test]
    fn content_is_checked_against_its_hash() {
        let hash = sha256_hex(b"echo hi\n");
        let sums = Sums(parse(&format!("{hash} *install.sh\n")));
        assert_eq!(sums.check("install.sh", b"echo hi\n"), Ok(()));
        assert!(sums.check("install.sh", b"echo bye\n").is_err());
    }

    #[test]
    fn malformed_lines_are_ignored() {
        let sums = parse(
            "short  a\nzz00000000000000000000000000000000000000000000000000000000000000  b\n",
        );
        assert!(sums.is_empty());
    }
}

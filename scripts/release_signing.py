"""Choose a release signing mode without silently dropping configured credentials."""

import os


def signing_mode(env):
    certificate = ("CERT_P12", "CERT_PASSWORD", "SIGN_IDENTITY")
    notary = ("NOTARY_KEY", "NOTARY_KEY_ID", "NOTARY_ISSUER")
    for group in (certificate, notary):
        present = [bool(env.get(key)) for key in group]
        if any(present) and not all(present):
            missing = ", ".join(key for key in group if not env.get(key))
            raise ValueError(f"Incomplete signing configuration; missing: {missing}")

    required = {}
    for key in ("REQUIRE_SIGNING", "REQUIRE_NOTARIZATION"):
        value = env.get(key, "").lower()
        if value not in ("", "false", "true"):
            raise ValueError(f"{key} must be true or false")
        required[key] = value == "true"

    has_certificate = all(env.get(key) for key in certificate)
    has_notary = all(env.get(key) for key in notary)
    if has_notary and not has_certificate:
        raise ValueError("Notarization requires certificate credentials")
    if required["REQUIRE_NOTARIZATION"] and not has_notary:
        raise ValueError("This repository requires notarization credentials")
    if required["REQUIRE_SIGNING"] and not has_certificate:
        raise ValueError("This repository requires a persistent signing certificate")

    return "notarized" if has_notary else "certificate" if has_certificate else "ad-hoc"


if __name__ == "__main__":
    try:
        mode = signing_mode(os.environ)
    except ValueError as error:
        raise SystemExit(str(error)) from None
    with open(os.environ["GITHUB_OUTPUT"], "a") as output:
        output.write(f"mode={mode}\n")
    print(f"Release signing: {mode}")

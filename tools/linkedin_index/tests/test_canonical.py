from linkedin_index.canonical import canonical_linkedin_url


def test_canonical_linkedin_url_normalizes_scheme_host_and_tracking():
    assert canonical_linkedin_url(
        "http://ca.linkedin.com/in/Andrew-Koumoudjian/?trk=public_profile#about"
    ) == "https://www.linkedin.com/in/Andrew-Koumoudjian"


def test_canonical_linkedin_url_preserves_encoded_slug_bytes():
    assert canonical_linkedin_url(
        "https://linkedin.com/in/a%2Fb%25c/"
    ) == "https://www.linkedin.com/in/a%2Fb%25c"


def test_canonical_linkedin_url_normalizes_unicode_and_percent_escape_case():
    unicode_url = canonical_linkedin_url(
        "https://www.linkedin.com/in/sérgio-faustino-0298981"
    )
    lowercase_escaped = canonical_linkedin_url(
        "https://www.linkedin.com/in/s%c3%a9rgio-faustino-0298981"
    )
    assert unicode_url == "https://www.linkedin.com/in/s%C3%A9rgio-faustino-0298981"
    assert lowercase_escaped == unicode_url


def test_canonical_linkedin_url_accepts_supported_entity_paths():
    assert canonical_linkedin_url(
        "https://www.linkedin.com/company/example-inc/"
    ) == "https://www.linkedin.com/company/example-inc"
    assert canonical_linkedin_url(
        "https://www.linkedin.com/school/concordia-university/"
    ) == "https://www.linkedin.com/school/concordia-university"


def test_canonical_linkedin_url_rejects_lookalike_hosts_and_unknown_paths():
    assert canonical_linkedin_url("https://evil-linkedin.com/in/example") is None
    assert canonical_linkedin_url("https://www.linkedin.com/feed/update/123") is None

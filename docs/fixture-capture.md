# Fixture capture guide

Capture only public provider responses using a test query with no personal data. Remove cookies, request IDs, IP addresses, tokens, VQD values and tracking identifiers. Store response bodies under `fixtures/engines/<id>/` and record synthetic status/content-type/redirect metadata separately.

Required cases are normal, empty, changed layout, access denied, rate limited, challenge, truncated, oversized, wrong content type and redirect.

Fixtures are test inputs, not evidence that a provider permits automated production access. Never commit a live CAPTCHA token or a response tied to an identifiable user session.

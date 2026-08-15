# Security policy

## Supported versions

There's only one branch, `main`. Whatever's there is what's supported, there are no version branches to patch separately.

## Reporting a vulnerability

If you find a security issue (auth bypass, SSRF, injection, key/secret exposure, anything that could affect someone's local instance or data), please **do not** open a public issue for it.

Use GitHub's private reporting instead: go to the **Security** tab on this repo → **Report a vulnerability**. That opens a private advisory only visible to me, so it doesn't get picked up before there's a fix.

This is a solo, unpaid side project, I don't make anything off it and I'm not a company with a bug bounty. If you're reporting something, be clear about what the actual issue is and how to reproduce it. I'll take real reports seriously and get a fix out, but I don't have time for vague reports, or worse, reports that come with attitude like I owe you something for finding it. I don't.

## Scope

Aetheria is meant to run locally or on infrastructure you control. Things like "the admin account can see all data" or "an unauthenticated user with shell access to the box can read the SQLite file" aren't vulnerabilities, that's the threat model of a self-hosted single-tenant-per-instance app. Cross-user data leakage, auth bypass, SSRF via the image proxy, and injection are all in scope.

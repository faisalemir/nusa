# Publishing `nusa/octane` to Packagist

The PHP driver lives in [`php-driver/`](../../php-driver/). Packagist publication is **operator-owned** (requires a Packagist account and tag).

## Preconditions

1. Composer package name in `php-driver/composer.json`: `nusa/octane`
2. Git tag on the commit that contains the driver (e.g. `php-driver-v0.1.0` or monorepo tag with subtree)
3. README install snippet matches [Laravel installation](../public/laravel/installation.md)

## Steps

1. **Tag** the release commit on GitHub.
2. **Register** at [packagist.org](https://packagist.org) → Submit package → point to the GitHub repo.
3. Enable **auto-update** (Packagist webhook) or run “Update” after each tag.
4. Verify: `composer require nusa/octane:^0.1` in a clean Laravel app.

## Monorepo note

If Packagist tracks the full `nusa` repository, set the package **install path** via Composer `extra` or publish a split branch that contains only `php-driver/` as root. Document the chosen layout in the release notes.

## Validation

- [ ] `composer validate` in `php-driver/`
- [ ] Laravel app boots with `NusaOctaneServiceProvider`
- [ ] Worker binary path documented in [php-driver](../public/laravel/php-driver.md)

# Changelog

## [0.1.1](https://github.com/sylver/mnemic/compare/v0.1.0...v0.1.1) (2026-11-26)


### Bug Fixes

* **ci:** Fix release publish GH_REPO and add tag republish dispatch ([#27](https://github.com/sylver/mnemic/issues/27)) ([a0f6bcb](https://github.com/sylver/mnemic/commit/a0f6bcbeb3a98622b7a9faf959082f1a8dae1129))

## 0.1.0 (2026-11-26)


### Features

* **check:** Add --fix for missing id and type ([#14](https://github.com/sylver/mnemic/issues/14)) ([227adf9](https://github.com/sylver/mnemic/commit/227adf96d8f951de1ed71e076f64a18cc6672712))
* **core:** Add check contract, note parser and vault gate ([#4](https://github.com/sylver/mnemic/issues/4)) ([9a2ab73](https://github.com/sylver/mnemic/commit/9a2ab73b27fca96fcd8558f96315eb87749ce208))
* **core:** Add crash-safe vault write lock ([#3](https://github.com/sylver/mnemic/issues/3)) ([3b7df56](https://github.com/sylver/mnemic/commit/3b7df5659dca679bae7af91f687f43c592c581e4))
* **core:** Add type pack, type checks and frontmatter editor ([#5](https://github.com/sylver/mnemic/issues/5)) ([4a898fb](https://github.com/sylver/mnemic/commit/4a898fb008a29f49b465872d9efdb5e3b7105203))
* **core:** Check vault-wide identity, links and hashes ([#8](https://github.com/sylver/mnemic/issues/8)) ([cf346b1](https://github.com/sylver/mnemic/commit/cf346b14b03d433ce06adb2e74e2185ab98c7330))
* **init:** Scaffold a vault without overwriting notes ([#7](https://github.com/sylver/mnemic/issues/7)) ([06532ba](https://github.com/sylver/mnemic/commit/06532ba36244d0ad4d269a68062755185736ccf4))
* **log:** Add mnemic log and confine the vault walk ([#10](https://github.com/sylver/mnemic/issues/10)) ([698534e](https://github.com/sylver/mnemic/commit/698534ec910ad739151baf394ad8f70cecfd4e6e))
* **mv:** Move and rename notes with alias and link rewrites ([#15](https://github.com/sylver/mnemic/issues/15)) ([4104c1c](https://github.com/sylver/mnemic/commit/4104c1c8af47301a126732c786b23e9fe3cd89ef))
* **new:** Create a note from the type template ([#13](https://github.com/sylver/mnemic/issues/13)) ([72736ac](https://github.com/sylver/mnemic/commit/72736acfbf8325a75742ae057aece430121f1164))
* **pack:** Templates and vault type overlays ([#6](https://github.com/sylver/mnemic/issues/6)) ([7e7ac75](https://github.com/sylver/mnemic/commit/7e7ac751c59c9e35b799dd1873e76633985f2244))
* **read:** Add mnemic get and mnemic links ([#9](https://github.com/sylver/mnemic/issues/9)) ([20e6689](https://github.com/sylver/mnemic/commit/20e6689446cbb404faaa6b6776d05ea301186f9f))
* Scaffold cargo workspace, ci and test harness ([#1](https://github.com/sylver/mnemic/issues/1)) ([0e9f93b](https://github.com/sylver/mnemic/commit/0e9f93bcf495293c73fc5eb3f10bd1bede6cf07b))
* **search:** Add keyword search over the vault scan ([#11](https://github.com/sylver/mnemic/issues/11)) ([db33867](https://github.com/sylver/mnemic/commit/db33867f72bf8a5ab76da56022a199f6b702f4c7))


### Documentation

* **contract:** Add init and mv envelopes and require one per command ([#18](https://github.com/sylver/mnemic/issues/18)) ([aef8b8f](https://github.com/sylver/mnemic/commit/aef8b8fcb59e776259a9c26b70ff3a6d49bb8970))
* Rewrite README for the v1 CLI with tests that keep it honest ([#17](https://github.com/sylver/mnemic/issues/17)) ([a3d141e](https://github.com/sylver/mnemic/commit/a3d141e62fdd5db8fbbb873329c22690cc7bb271))


### Tests

* **confined_io:** Exempt sanctioned helpers only in their defining file ([#16](https://github.com/sylver/mnemic/issues/16)) ([c5c7638](https://github.com/sylver/mnemic/commit/c5c763856741b23aaca80c4520d3583b1af88436))
* **core:** Enforce confined vault IO ([#12](https://github.com/sylver/mnemic/issues/12)) ([0aa0514](https://github.com/sylver/mnemic/commit/0aa051482f16ca4c1f7dcd3b7547da1924d0b953))


### Chores

* **release:** Customize release-please changelog and PR format ([#25](https://github.com/sylver/mnemic/issues/25)) ([976a238](https://github.com/sylver/mnemic/commit/976a238f81820102360a4b6f29902f52ade7aa91))
* **release:** Hide default release-please PR footer ([#26](https://github.com/sylver/mnemic/issues/26)) ([f48de46](https://github.com/sylver/mnemic/commit/f48de46a3ca362476e94bcfac7ae182bbea6ecc9))


### Continuous Integration

* Automate version bump, changelog, tag, and artifact publish via release-please ([#21](https://github.com/sylver/mnemic/issues/21)) ([be8a8e0](https://github.com/sylver/mnemic/commit/be8a8e049b0eff33ee2cd18330fb2a29f9341e88))
* Enforce conventional commits and harden release checks ([#20](https://github.com/sylver/mnemic/issues/20)) ([b387d6a](https://github.com/sylver/mnemic/commit/b387d6a52cc5589c48ef7ebccfeeda91b0e112b4))
* Pin GitHub Actions to full commit SHAs ([#22](https://github.com/sylver/mnemic/issues/22)) ([d63e796](https://github.com/sylver/mnemic/commit/d63e796317eeb386c70e0ea6ccbde40a1610dccc))
* **release:** Build macOS arm64 and Linux musl binaries on version tags ([#19](https://github.com/sylver/mnemic/issues/19)) ([726ea4a](https://github.com/sylver/mnemic/commit/726ea4a49aa8a89fdecbfb1a8d22fe0cd8d3f65e))
* Scope release workflow to releases only ([#24](https://github.com/sylver/mnemic/issues/24)) ([8074001](https://github.com/sylver/mnemic/commit/8074001c3190a40c9bb97781b303925b834c70a4))

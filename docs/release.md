# 发布

版本只写在 `Cargo.toml` 的 `workspace.package.version`。Git 标签是 `v` 加上这个版本，例如 `0.1.0` 对应 `v0.1.0`。`packaging/arch/PKGBUILD` 的 `pkgver` 和 AppStream `<release version>` 要和它相同。

发布构建命令：

```bash
cargo build --release --locked -p qingyin-ui-bridge --bin qingyin
```

`packaging/install.sh` 会在这条命令之外加上 `--remap-path-prefix`，避免发布的二进制带上构建机的家目录路径。手动编译时也要加上同样的参数。

QML 已经嵌在可执行文件里。安装脚本把文件放到 `${DESTDIR}${PREFIX}` 下，默认前缀是 `/usr/local`：

```bash
PREFIX=/usr/local bash packaging/install.sh
QT_QPA_PLATFORM=offscreen qingyin --smoke
```

## 本地检查 Arch 包

不需要 yay。在本机：

```bash
cd packaging/arch
makepkg -si
namcap PKGBUILD
namcap qingyin-*.pkg.tar.zst
```

干净的 Arch 构建环境用官方 `devtools`，同样不需要 AUR helper：

```bash
sudo pacman -S devtools
cd packaging/arch
extra-x86_64-build
```

较新的 devtools 也可以在 `packaging/arch` 里执行 `pkgctl build`。

装好之后确认包里只有这四个路径，没有 `target/` 或源码目录：

```bash
pacman -Qlp qingyin-*.pkg.tar.zst
```

## 推送标签

`main` 干净且 CI 通过之后：

```bash
git tag -a v0.1.0 -m "Qingyin v0.1.0"
git push origin v0.1.0
```

推送 `v*` 标签会运行 `.github/workflows/release.yml`：它校验这次标签，在干净的 Arch 容器里用非 root 的 `makepkg` 构建，再用 `GITHUB_TOKEN` 创建 GitHub Release，并附上 `.pkg.tar.zst`。

`v0.1.0` 已经发布，而且不包含这套打包文件。不要移动那个标签。`v0.1.1` 是第一版带 Arch 打包和 GitHub Release 工作流的标签。再往后发布时，把 `Cargo.toml`、`pkgver` 和 metainfo 一起改成新版本，然后推送对应的 `v*` 标签。

标签对应的 GitHub 源码包校验和要在标签已经存在之后才能计算。Release 工作流会用实际下载的源码包替换 `PKGBUILD` 里的第一枚 `sha256sums` 再运行 `makepkg`。工作流日志里的 `GitHub archive sha256` 就是 AUR 包应使用的校验和。

## 更新 AUR

不要把 AUR 密码、SSH 私钥或 token 放进这个仓库或 GitHub Actions。本仓库不自动推送 AUR。

AUR 仓库是 `ssh://aur@aur.archlinux.org/qingyin.git`。它和这份源码仓库分开。至少放进：

- `PKGBUILD`
- `.SRCINFO`
- `io.github.nook001.qingyin.desktop`
- `io.github.nook001.qingyin.metainfo.xml`
- `io.github.nook001.qingyin.svg`

`.SRCINFO` 在 `packaging/arch` 里生成，不要手改：

```bash
cd packaging/arch
makepkg --printsrcinfo > .SRCINFO
```

先用上面的 `makepkg` 或 `extra-x86_64-build` 确认包可用，再把这五个文件提交到 AUR 仓库并推送。

仓库目前没有选择软件许可证。AUR 审核通常会要求补上许可证；在那之前不要把包标成某个开源许可证。

# NoRisk Client Launcher (Fork)

This repository is a fork of the official NoRisk Client Launcher. You can compile it yourself or download the latest prebuilt installer from the Releases page. Support for the NoRisk Client is not included yet, but it is upcoming.

- Build it yourself: see Compile it yourself below
- Or download the newest version from Releases once published

## Issues

If you notice any bugs or missing features, you can let us know by opening an issue [here](https://github.com/NoRiskClient/issues/issues/new/choose).

## License
This code is originally forked and still uses a small amount of code from [LiquidLauncher](https://github.com/CCBlueX/LiquidLauncher).

Therefore, this project is also subject to the [GNU General Public License v3.0](LICENSE). This does only apply for source code located directly in this clean repository. During the development and compilation process, additional source code may be used to which we have obtained no rights. Such code is not covered by the GPL license.
This project entirely or partially for free and even commercially. However, please consider the following:

- **You must disclose the source code of your modified work and the source code you took from this project. This means you are not allowed to use code from this project (even partially) in a closed-source (or even obfuscated) application.**
- **Your modified application must also be licensed under the GPL** 

Do the above and share your source code with everyone; just like we do.

## Compile it yourself!

### Prerequisites
- **Node.js** (v18 or higher) - [Download here](https://nodejs.org/en/download)
- **Rust** (latest stable) - [Install here](https://www.rust-lang.org/tools/install)
- **Yarn** package manager - `npm install -g yarn`

### Setup Instructions
1. Clone the repository:
   ```bash
   git clone --recurse-submodules https://github.com/NoRiskClient/noriskclient-launcher
   cd noriskclient-launcher
   ```

2. Install dependencies:
   ```bash
   yarn install
   ```

3. Start development mode:
   ```bash
   yarn tauri dev
   ```

4. Build for production:
   ```bash
   yarn tauri build
   ```

## Contributing
We appreciate contributions. So if you want to support us, feel free to make changes to NoRisk source code and submit a pull request.

# dioxus-tailwind-template

> a template for starting a dioxus project to be used with [dioxus-cli](https://github.com/DioxusLabs/cli) and tailwindcss

## Usage

#### Install necessary binaries

```
cargo install --force sqlx-cli cargo-watch dioxus-cli systemfd wasm-bindgen-cli
```

#### use `dioxus-cli` to init the template

```
dioxus init hello-dioxus --template=gh:casualjim/dioxus-tailwind-template
```

#### Update the session secret for the backend

```
cp .env.example .env
echo SESSION_SECRET=\"$(openssl rand -base64 128 | tr -d '\n')\" >> .env
```

#### Start a `dev-server` for the project

in terminal 1: 

```
cd ./hello-dioxus
cargo frontend
```

in terminal 2:

```
cargo backend
```

Then open a browser: https://localhost:8443

or package this project:

```
dioxus build --release
```

## Project Structure

```
.project
- public # save the assets you want include in your project.
- src # put your code
- - utils # save some public function
- - components # save some custom components
```

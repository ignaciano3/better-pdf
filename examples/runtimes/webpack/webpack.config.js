const path = require("path");

/** @type {import('webpack').Configuration} */
module.exports = {
  entry: "./src/index.js",
  output: {
    filename: "bundle.js",
    path: path.resolve(__dirname, "dist"),
    clean: true,
  },
  // Required for async WASM loading
  experiments: {
    asyncWebAssembly: true,
  },
  module: {
    rules: [
      {
        // Emit the .wasm file as a separate asset and give us a URL to it
        test: /\.wasm$/,
        type: "asset/resource",
      },
    ],
  },
  resolve: {
    extensions: [".js"],
  },
};

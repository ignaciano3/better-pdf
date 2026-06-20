/** @type {import('next').NextConfig} */
const nextConfig = {
  // Suppress the "Can't resolve '*.wasm'" warning from webpack in SSR paths —
  // the wasm is served from public/ and loaded client-side only.
  webpack(config, { isServer }) {
    if (!isServer) {
      // .wasm files in node_modules are not automatically emitted by Next.js;
      // we serve the file from public/ instead (see README for the copy step).
      // No additional rule is needed — initializeWasm() fetches from a URL.
    }
    return config;
  },
};

module.exports = nextConfig;

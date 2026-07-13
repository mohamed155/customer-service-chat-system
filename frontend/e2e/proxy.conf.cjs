const backendPort = process.env.E2E_BACKEND_PORT || '18080';

module.exports = {
  '/api': {
    target: `http://127.0.0.1:${backendPort}`,
    secure: false,
  },
};

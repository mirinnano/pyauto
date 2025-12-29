/** @type {import('tailwindcss').Config} */
export default {
    content: [
        "./index.html",
        "./src/**/*.{js,ts,jsx,tsx}",
    ],
    theme: {
        extend: {
            colors: {
                dark: {
                    inner: '#0a0a0a',
                    accent: '#1a1a1a',
                },
                primary: {
                    glow: '#00ccff',
                }
            },
            animation: {
                'glow': 'glow 2s ease-in-out infinite alternate',
            },
            keyframes: {
                glow: {
                    'from': { 'box-shadow': '0 0 10px #00ccff, 0 0 20px #00ccff' },
                    'to': { 'box-shadow': '0 0 20px #00ccff, 0 0 30px #00ccff' },
                }
            }
        },
    },
    plugins: [],
}

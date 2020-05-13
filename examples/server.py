import http.server
from http.server import HTTPServer, BaseHTTPRequestHandler
import socketserver

PORT = 8000

Handler = http.server.SimpleHTTPRequestHandler

Handler.extensions_map = {
    '.manifest':    'text/cache-manifest',
    '.html':        'text/html',
    '.png':         'image/png',
    '.jpg':         'image/jpg',
    '.svg':         'image/svg+xml',
    '.css':         'text/css',
    '.js':          'application/x-javascript',
    '.wasm':        'application/wasm',
    '':             'application/octet-stream',
}


class MyHTTPRequestHandler(http.server.SimpleHTTPRequestHandler):
    def end_headers(self):
        self.send_my_headers()
        http.server.SimpleHTTPRequestHandler.end_headers(
            self)

    def send_my_headers(self):
        self.send_header(
            "Cache-Control", "no-cache, no-store, must-revalidate")
        self.send_header("Pragma", "no-cache")
        self.send_header("Expires", "0")


httpd = socketserver.TCPServer(("", PORT), MyHTTPRequestHandler)

print("serving at port", PORT)
httpd.serve_forever()

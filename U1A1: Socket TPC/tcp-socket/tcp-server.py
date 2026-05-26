import socket

HOST = '127.0.0.1'
PORT = 8888

# Creación del socket TCP (SOCK_STREAM = orientado a conexión)
sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
sock.bind((HOST, PORT))
sock.listen(1)  # Máximo 1 conexión en cola

print(f'[TCP] Servidor escuchando en {HOST}:{PORT}')

conn, direccion = sock.accept()  # Bloquea hasta que un cliente se conecta
print(f'[TCP] Conexión establecida con {direccion}')

datos = conn.recv(1024)
print(f'[TCP] Mensaje recibido: {datos.decode()}')
conn.sendall(b'Mensaje recibido (TCP)')

conn.close()
sock.close()

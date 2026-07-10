import socket

HOST = '127.0.0.1'
PORT = 9999

# Creación del socket UDP (SOCK_DGRAM = no orientado a conexión)
sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
sock.bind((HOST, PORT))

print(f'[UDP] Servidor escuchando en {HOST}:{PORT}')

while True:
    datos, direccion = sock.recvfrom(1024)  # Recibe hasta 1024 bytes
    print(f'[UDP] Mensaje de {direccion}: {datos.decode()}')
    sock.sendto(b'Mensaje recibido (UDP)', direccion)  # Responde al cliente

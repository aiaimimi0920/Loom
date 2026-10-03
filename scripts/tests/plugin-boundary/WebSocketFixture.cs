using System;
using System.IO;
using System.Net;
using System.Net.Sockets;
using System.Security.Cryptography;
using System.Text;
using System.Threading;
using System.Threading.Tasks;

// Loopback-only RFC 6455 fixture; no HttpListener URL ACL or daemon required.
public sealed class PluginBoundarySocketFixture : IDisposable
{
    readonly TcpListener listener;
    readonly CancellationTokenSource stop = new CancellationTokenSource();
    readonly Task worker;
    TcpClient client;
    public readonly int Port;
    public int ReceivedMessages;

    public PluginBoundarySocketFixture(string scenario)
    {
        listener = new TcpListener(IPAddress.Loopback, 0);
        listener.Start();
        Port = ((IPEndPoint)listener.LocalEndpoint).Port;
        worker = Task.Run(() => Run(scenario));
    }

    void Run(string scenario)
    {
        try
        {
            client = listener.AcceptTcpClient();
            client.ReceiveTimeout = 5000;
            client.SendTimeout = 5000;
            using (NetworkStream stream = client.GetStream())
            {
                var header = new StringBuilder();
                while (!header.ToString().EndsWith("\r\n\r\n", StringComparison.Ordinal))
                {
                    int b = stream.ReadByte();
                    if (b < 0 || header.Length > 8192) return;
                    header.Append((char)b);
                }
                if (scenario == "connect-timeout") { stop.Token.WaitHandle.WaitOne(5000); return; }
                string key = null;
                foreach (string line in header.ToString().Split(new[] { "\r\n" }, StringSplitOptions.None))
                    if (line.StartsWith("Sec-WebSocket-Key:", StringComparison.OrdinalIgnoreCase))
                        key = line.Substring(line.IndexOf(':') + 1).Trim();
                using (var sha = SHA1.Create())
                {
                    string accept = Convert.ToBase64String(sha.ComputeHash(Encoding.ASCII.GetBytes(key + "258EAFA5-E914-47DA-95CA-C5AB0DC85B11")));
                    byte[] response = Encoding.ASCII.GetBytes("HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: " + accept + "\r\n\r\n");
                    stream.Write(response, 0, response.Length);
                }
                if (scenario == "send-timeout" || scenario == "receive-timeout") { stop.Token.WaitHandle.WaitOne(5000); return; }
                if (scenario == "fragment-timeout") { Frame(stream, 1, "{\"secret\":\"", false); stop.Token.WaitHandle.WaitOne(5000); return; }
                if (scenario == "close") { Frame(stream, 8, "", true); return; }
                if (scenario == "bad-json") { Frame(stream, 1, "private-payload-not-json", true); return; }
                if (scenario == "unrelated")
                {
                    Frame(stream, 1, "{\"method\":\"loom.hook.art.progress\",\"private\":\"private-payload\"}", true);
                    Frame(stream, 1, "{\"protocolVersion\":\"loom.hook.v1\",\"requestId\":\"other\",\"status\":\"succeeded\"}", true);
                    Frame(stream, 1, "{\"protocolVersion\":\"loom.hook.v1\",\"requestId\":\"execute:third-party-plugin\",\"status\":\"succeeded\"}", true);
                }
                else
                {
                    ReadClientMessage(stream);
                    Interlocked.Increment(ref ReceivedMessages);
                    Frame(stream, 1, "{\"status\":", false);
                    Frame(stream, 0, "\"succeeded\"}", true);
                }
                stop.Token.WaitHandle.WaitOne(5000);
            }
        }
        catch (IOException) { }
        catch (SocketException) { }
        catch (ObjectDisposedException) { }
    }

    static void ReadClientMessage(Stream stream)
    {
        if (stream.ReadByte() < 0) throw new IOException();
        int flags = stream.ReadByte();
        int length = flags & 127;
        if (flags < 0 || length > 125 || (flags & 128) == 0) throw new IOException();
        for (int index = 0; index < length + 4; index++)
            if (stream.ReadByte() < 0) throw new IOException();
    }

    static void Frame(Stream stream, int opcode, string text, bool final)
    {
        byte[] data = Encoding.UTF8.GetBytes(text);
        if (data.Length > 125) throw new InvalidOperationException();
        stream.WriteByte((byte)(opcode | (final ? 128 : 0)));
        stream.WriteByte((byte)data.Length);
        stream.Write(data, 0, data.Length);
        stream.Flush();
    }

    public void Dispose()
    {
        stop.Cancel();
        listener.Stop();
        if (client != null) client.Close();
        if (!worker.Wait(6000)) throw new TimeoutException("Loopback fixture did not stop.");
        stop.Dispose();
    }
}

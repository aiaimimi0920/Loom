using System;
using System.Reflection;
using System.Text.RegularExpressions;
#if VERSION2
[assembly: AssemblyVersion("1.1.0.0")]
#else
[assembly: AssemblyVersion("1.0.0.0")]
#endif
// Portable text normalization app: v2 collapses consecutive whitespace in addition to trimming.
class TextNormalize {
    static int Main(string[] args) {
        if (args.Length != 1) return 2;
        string text = args[0].Trim();
#if VERSION2
        text = Regex.Replace(text, @"\s+", " ");
#endif
        Console.Write(text);
        return 0;
    }
}

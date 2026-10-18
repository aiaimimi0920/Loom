using System;
using System.Reflection;
using System.Text.RegularExpressions;
#if VERSION2
[assembly: AssemblyVersion("1.1.0.0")]
#else
[assembly: AssemblyVersion("1.0.0.0")]
#endif
// 便携文本规范化应用：新版在去掉首尾空白之外合并连续空白。
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

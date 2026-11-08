param(
    [Parameter(Mandatory = $true)][string]$Reference,
    [Parameter(Mandatory = $true)][string]$Actual,
    [Parameter(Mandatory = $true)][string]$Output
)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
Add-Type -ReferencedAssemblies System.Drawing -TypeDefinition @'
using System;
using System.Drawing;
public static class HtmlPreviewPixels {
    public static double[] Compare(string reference, string actual) {
        using (var a = new Bitmap(reference))
        using (var b = new Bitmap(actual)) {
            if (a.Width != b.Width || Math.Abs(a.Height - b.Height) > 1)
                throw new Exception("Only a one-row DPR rounding difference is allowed");
            int height = Math.Min(a.Height, b.Height);
            long changed = 0, total = 0;
            int maximum = 0;
            for (int y = 0; y < height; y++) {
                for (int x = 0; x < a.Width; x++) {
                    var p = a.GetPixel(x, y);
                    var q = b.GetPixel(x, y);
                    int r = Math.Abs(p.R - q.R), g = Math.Abs(p.G - q.G), blue = Math.Abs(p.B - q.B);
                    if (r + g + blue > 0) changed++;
                    total += r + g + blue;
                    maximum = Math.Max(maximum, Math.Max(r, Math.Max(g, blue)));
                }
            }
            long pixels = (long)a.Width * height;
            return new double[] { a.Width, height, pixels, changed,
                100.0 * changed / pixels, (double)total / (pixels * 3), maximum,
                a.Height, b.Height };
        }
    }
}
'@
$values = [HtmlPreviewPixels]::Compare((Resolve-Path -LiteralPath $Reference), (Resolve-Path -LiteralPath $Actual))
$result = [ordered]@{
    width = $values[0]
    height = $values[1]
    pixels = $values[2]
    changed_pixels = $values[3]
    changed_pixel_percent = $values[4]
    mean_absolute_rgb_error_0_to_255 = $values[5]
    maximum_channel_error_0_to_255 = $values[6]
    reference_height = $values[7]
    actual_height = $values[8]
}
$json = ($result | ConvertTo-Json) -replace [char]13, ''
[IO.File]::WriteAllText($Output, $json + [char]10, [Text.UTF8Encoding]::new($false))
$result

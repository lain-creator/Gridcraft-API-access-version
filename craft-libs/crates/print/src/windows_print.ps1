# Print a print-ready PDF on Windows (CUPS-independent backend). The job arrives in CRAFT_* environment
# variables, never as arguments: printer names contain spaces and quotes, and the document's
# path must not be visible in the process list while it prints.
#
# The in-box Windows.Data.Pdf renders each sheet at the printer's own resolution and
# System.Drawing.Printing spools it with the driver's settings — copies, collation, duplex and
# colour reach the spooler as the job's, the way `lp -o` sends them on CUPS. No PDF reader is
# needed, and the calling code touches no Win32 API.
#
# CRAFT_PRINT_DRYRUN=1 renders every sheet in memory and prints
# nothing: the tests run the whole rendering path on machines without a printer, CI included.
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = New-Object System.Text.UTF8Encoding($false)

Add-Type -AssemblyName System.Drawing
# Windows duplex values are binding edges: Vertical = long edge, Horizontal = short edge.
# https://learn.microsoft.com/en-us/windows/win32/api/wingdi/ns-wingdi-devmodea
function Get-Duplex($edge) {
    switch ($edge) {
        'long-edge' { [System.Drawing.Printing.Duplex]::Vertical }
        'short-edge' { [System.Drawing.Printing.Duplex]::Horizontal }
        default { [System.Drawing.Printing.Duplex]::Simplex }
    }
}

function Set-JobDuplex($settings, $edge) {
    if ($edge) { $settings.Duplex = Get-Duplex $edge }
}

# WinRT dimensions are DIPs (96/inch), printing dimensions are hundredths of an inch.
# The PDF is already composed: preserve physical size, even if the printer clips its edges.
function Get-SheetGeometry($width, $height) {
    if ([double]::IsNaN($width) -or [double]::IsNaN($height) -or
        [double]::IsInfinity($width) -or [double]::IsInfinity($height) -or
        $width -le 0 -or $height -le 0 -or $width -gt 12288 -or $height -gt 12288) {
        throw 'the PDF sheet dimensions are invalid or exceed 128 inches'
    }
    # DEVMODE paper lengths use signed 16-bit tenths of a millimetre (128 inches fits).
    $drawW = $width * 100.0 / 96.0
    $drawH = $height * 100.0 / 96.0
    [pscustomobject]@{
        Width = $drawW; Height = $drawH; Landscape = ($width -gt $height)
        PaperWidth = [int][Math]::Max(1, [Math]::Round([Math]::Min($drawW, $drawH)))
        PaperHeight = [int][Math]::Max(1, [Math]::Round([Math]::Max($drawW, $drawH)))
    }
}

function Set-SheetSettings($settings, $papers, $sheet, $scaling = 'exact', $fallbackPaper = $null) {
    # Prefer the driver's paper identifier over a custom form when the dimensions match.
    $paper = $papers | Where-Object {
        [Math]::Abs($_.Width - $sheet.PaperWidth) -le 1 -and
        [Math]::Abs($_.Height - $sheet.PaperHeight) -le 1
    } | Select-Object -First 1
    if (-not $paper -and $scaling -eq 'shrink-to-fit') {
        if (-not $fallbackPaper) { throw 'the printer has no default paper for shrink-to-fit' }
        $paper = $fallbackPaper
    }
    if (-not $paper) {
        $paper = [System.Drawing.Printing.PaperSize]::new('PDF sheet', $sheet.PaperWidth, $sheet.PaperHeight)
    }
    $settings.PaperSize = $paper
    $settings.Landscape = $sheet.Landscape
    $settings.Margins = [System.Drawing.Printing.Margins]::new(0, 0, 0, 0)
}

function Get-DrawBox($sheet, $hardMarginX, $hardMarginY) {
    # GDI's origin is the printable area's corner. Offset to the physical paper's corner.
    [System.Drawing.RectangleF]::new(-$hardMarginX, -$hardMarginY, $sheet.Width, $sheet.Height)
}

function Get-ShrinkDrawBox($sheet, $area, $hardMarginX, $hardMarginY) {
    foreach ($value in @($area.X, $area.Y, $area.Width, $area.Height, $hardMarginX, $hardMarginY)) {
        if ([double]::IsNaN($value) -or [double]::IsInfinity($value)) {
            throw 'the printer reported an invalid printable area'
        }
    }
    if ($area.Width -le 0 -or $area.Height -le 0) { throw 'the printer has no printable area' }
    $scale = [Math]::Min(1.0, [Math]::Min($area.Width / $sheet.Width, $area.Height / $sheet.Height))
    $width = $sheet.Width * $scale
    $height = $sheet.Height * $scale
    [System.Drawing.RectangleF]::new(
        $area.X + ($area.Width - $width) / 2.0 - $hardMarginX,
        $area.Y + ($area.Height - $height) / 2.0 - $hardMarginY,
        $width, $height)
}

function Get-RenderSize($sheet, $dpi) {
    $dpi = [Math]::Min(600, [Math]::Max(72, $dpi))
    $w = [Math]::Ceiling($sheet.Width / 100.0 * $dpi)
    $h = [Math]::Ceiling($sheet.Height / 100.0 * $dpi)
    # Bound allocation without changing the physical draw rectangle.
    $scale = [Math]::Min(1.0, [Math]::Min(32768.0 / [Math]::Max($w, $h), [Math]::Sqrt(64000000.0 / ($w * $h))))
    [pscustomobject]@{ Width = [int][Math]::Max(1, [Math]::Floor($w * $scale)); Height = [int][Math]::Max(1, [Math]::Floor($h * $scale)) }
}

# Exercise the production settings/placement helpers without an installed printer. The
# caller clears this internal test flag for ordinary submissions.
if ($env:CRAFT_PRINT_TEST_SETTINGS -eq '1') {
    $papers = @([System.Drawing.Printing.PaperSize]::new('Driver Letter', 850, 1100))
    $results = foreach ($size in @(@(816, 1056), @(1056, 816), @(793.92, 1122.24), @(816, 1344))) {
        $sheet = Get-SheetGeometry $size[0] $size[1]
        $settings = [System.Drawing.Printing.PageSettings]::new()
        Set-SheetSettings $settings $papers $sheet
        $box = Get-DrawBox $sheet 25 30
        $pixels = Get-RenderSize $sheet 150
        [pscustomobject]@{ Width = $settings.PaperSize.Width; Height = $settings.PaperSize.Height;
            Name = $settings.PaperSize.PaperName; Landscape = $settings.Landscape;
            Margins = @($settings.Margins.Left, $settings.Margins.Right, $settings.Margins.Top, $settings.Margins.Bottom);
            X = $box.X; Y = $box.Y; DrawWidth = $box.Width; DrawHeight = $box.Height;
            Pixels = @($pixels.Width, $pixels.Height) }
    }
    $large = Get-RenderSize (Get-SheetGeometry 12288 12288) 600
    $invalidRejected = $false
    try { $null = Get-SheetGeometry ([double]::NaN) 100 } catch { $invalidRejected = $true }
    $fallback = [System.Drawing.Printing.PageSettings]::new()
    Set-SheetSettings $fallback $papers (Get-SheetGeometry 1536 1920) 'shrink-to-fit' $papers[0]
    $area = [System.Drawing.RectangleF]::new(25, 30, 800, 1040)
    $shrinkBoxes = foreach ($size in @(@(1536, 1920), @(384, 480))) {
        $box = Get-ShrinkDrawBox (Get-SheetGeometry $size[0] $size[1]) $area 25 30
        ,@($box.X, $box.Y, $box.Width, $box.Height)
    }
    $defaultDuplex = [pscustomobject]@{ Duplex = [System.Drawing.Printing.Duplex]::Vertical }
    Set-JobDuplex $defaultDuplex ''
    $preservedDuplex = $defaultDuplex.Duplex.ToString()
    Set-JobDuplex $defaultDuplex 'off'
    [pscustomobject]@{ Sheets = @($results); LongEdge = (Get-Duplex 'long-edge').ToString();
        ShortEdge = (Get-Duplex 'short-edge').ToString(); Off = (Get-Duplex 'off').ToString();
        DefaultDuplex = $preservedDuplex; ForcedSimplex = $defaultDuplex.Duplex.ToString();
        FallbackPaper = $fallback.PaperSize.PaperName; ShrinkBoxes = @($shrinkBoxes);
        LargePixels = @($large.Width, $large.Height); InvalidRejected = $invalidRejected } | ConvertTo-Json -Depth 5 -Compress
    return
}

Add-Type -AssemblyName System.Runtime.WindowsRuntime
try {
    $null = [Windows.Data.Pdf.PdfDocument, Windows.Data.Pdf, ContentType = WindowsRuntime]
} catch {
    throw 'this Windows installation cannot render PDFs: the Windows.Data.Pdf component is missing'
}
$null = [Windows.Storage.StorageFile, Windows.Storage, ContentType = WindowsRuntime]

# Await the WinRT asynchronous APIs from .NET Framework's PowerShell. AsTask is an extension
# method: found by reflecting over the static class — with `.`, an instance call on the type
# object, since `GetMethods` is not static and `::` would not find it.
$type = [System.WindowsRuntimeSystemExtensions]
$opTask = $type.GetMethods() | Where-Object {
    $_.Name -eq 'AsTask' -and $_.GetParameters().Count -eq 1 -and $_.GetParameters()[0].ParameterType.Name -eq 'IAsyncOperation`1'
} | Select-Object -First 1
$actTask = $type.GetMethods() | Where-Object {
    $_.Name -eq 'AsTask' -and $_.GetParameters().Count -eq 1 -and $_.GetParameters()[0].ParameterType.Name -eq 'IAsyncAction'
} | Select-Object -First 1
function AwaitOp($operation, $resultType) {
    $task = $opTask.MakeGenericMethod($resultType).Invoke($null, @($operation))
    $task.Wait()
    $task.Result
}
function AwaitAct($action) {
    $task = $actTask.Invoke($null, @($action))
    $task.Wait()
}

# Render one sheet at its physical size; only raster resolution can be reduced for memory.
function Render-Sheet($page, $sheet, $dpi) {
    $pixels = Get-RenderSize $sheet $dpi
    $options = [Windows.Data.Pdf.PdfPageRenderOptions]::new()
    $options.DestinationWidth = $pixels.Width
    $options.DestinationHeight = $pixels.Height
    $stream = [System.IO.MemoryStream]::new()
    try {
        $random = [System.IO.WindowsRuntimeStreamExtensions]::AsRandomAccessStream($stream)
        AwaitAct ($page.RenderToStreamAsync($random, $options))
        $stream.Position = 0
        $png = [System.Drawing.Image]::FromStream($stream)
        try { return [System.Drawing.Bitmap]::new($png) } finally { $png.Dispose() }
    } finally { $stream.Dispose() }
}

$source = AwaitOp ([Windows.Storage.StorageFile]::GetFileFromPathAsync($env:CRAFT_PRINT_FILE)) ([Windows.Storage.StorageFile])
$document = AwaitOp ([Windows.Data.Pdf.PdfDocument]::LoadFromFileAsync($source)) ([Windows.Data.Pdf.PdfDocument])
if ($document.PageCount -lt 1) { throw 'the print-ready PDF has no pages' }

if ($env:CRAFT_PRINT_DRYRUN -eq '1') {
    # Render at actual sheet dimensions without touching a printer or leaving PNG files.
    for ($i = 0; $i -lt $document.PageCount; $i++) {
        $page = $document.GetPage($i)
        try {
            $sheet = Get-SheetGeometry $page.Size.Width $page.Size.Height
            $pixels = Get-RenderSize $sheet 150
            $bitmap = Render-Sheet $page $sheet 150
            try {
                if ($bitmap.Width -ne $pixels.Width -or $bitmap.Height -ne $pixels.Height) {
                    throw 'the PDF renderer changed the requested sheet dimensions'
                }
            } finally { $bitmap.Dispose() }
        } finally { $page.Dispose() }
    }
    Write-Output ('rendered ' + $document.PageCount + ' sheet(s)')
    return
}

$printer = [System.Drawing.Printing.PrintDocument]::new()
$printer.DocumentName = $env:CRAFT_PRINT_TITLE
$settings = $printer.PrinterSettings
if ($env:CRAFT_PRINT_PRINTER) {
    try {
        $settings.PrinterName = $env:CRAFT_PRINT_PRINTER
    } catch {
        throw ('the printer is not available: ' + $env:CRAFT_PRINT_PRINTER)
    }
}
$settings.Copies = [Math]::Min(32767, [Math]::Max(1, [int]$env:CRAFT_PRINT_COPIES))
$settings.Collate = ($env:CRAFT_PRINT_COLLATE -eq '1')
Set-JobDuplex $settings $env:CRAFT_PRINT_DUPLEX
if ($env:CRAFT_PRINT_GRAYSCALE -eq '1') { $printer.DefaultPageSettings.Color = $false }
$fallbackPaper = $printer.DefaultPageSettings.PaperSize

# QueryPageSettings runs before each PrintPage, allowing mixed sizes/orientations in one job.
# https://learn.microsoft.com/en-us/dotnet/api/system.drawing.printing.printdocument.querypagesettings
$printer.OriginAtMargins = $false
$printer.PrintController = [System.Drawing.Printing.StandardPrintController]::new()
$script:sheets = 0
$printer.add_QueryPageSettings({
    param($sender, $e)
    $page = $document.GetPage($script:sheets)
    try {
        $sheet = Get-SheetGeometry $page.Size.Width $page.Size.Height
        Set-SheetSettings $e.PageSettings $sender.PrinterSettings.PaperSizes $sheet $env:CRAFT_PRINT_SCALING $fallbackPaper
    } finally { $page.Dispose() }
})
$printer.add_PrintPage({
    param($sender, $e)
    $page = $document.GetPage($script:sheets)
    $bitmap = $null
    try {
        $sheet = Get-SheetGeometry $page.Size.Width $page.Size.Height
        if ($env:CRAFT_PRINT_SCALING -ne 'shrink-to-fit' -and
            ([Math]::Abs($e.PageBounds.Width - $sheet.Width) -gt 2 -or
             [Math]::Abs($e.PageBounds.Height - $sheet.Height) -gt 2)) {
            throw 'the printer cannot use the PDF sheet size; select matching paper and try again'
        }
        $bitmap = Render-Sheet $page $sheet $e.Graphics.DpiX
        if ($env:CRAFT_PRINT_SCALING -eq 'shrink-to-fit') {
            $box = Get-ShrinkDrawBox $sheet $e.PageSettings.PrintableArea $e.PageSettings.HardMarginX $e.PageSettings.HardMarginY
        } else {
            $box = Get-DrawBox $sheet $e.PageSettings.HardMarginX $e.PageSettings.HardMarginY
        }
        $e.Graphics.DrawImage($bitmap, $box)
    } finally {
        if ($bitmap) { $bitmap.Dispose() }
        $page.Dispose()
    }
    $script:sheets++
    $e.HasMorePages = ($script:sheets -lt $document.PageCount)
})
try {
    $printer.Print()
    Write-Output ('spooled ' + $document.PageCount + ' sheet(s)')
} finally { $printer.Dispose() }

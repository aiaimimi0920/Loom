<# Focused contract for generic paired-candidate compatibility evidence. #>

Assert-ScriptContract `
    -Path @($extensionCompatibilityPath) `
    -RequiredText @(
        'loom.extension.v1',
        'loom.capability.package.v1',
        'declarative.v1',
        'function Assert-LoomExtensionCompatibility'
    ) `
    -ForbiddenText @('neuro.official/ocr')

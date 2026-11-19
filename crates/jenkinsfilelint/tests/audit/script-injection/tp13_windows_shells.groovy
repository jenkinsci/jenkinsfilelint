node('windows') {
    bat "git checkout ${env.CHANGE_BRANCH}"
    powershell "Write-Output '${env.CHANGE_TITLE}'"
    pwsh script: "Write-Output ${params.MESSAGE}"
}

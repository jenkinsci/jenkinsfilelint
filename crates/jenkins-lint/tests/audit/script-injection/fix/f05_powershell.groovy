pipeline {
    agent any
    stages {
        stage('S') {
            steps {
                powershell "Write-Output ${env.CHANGE_TITLE}"
                pwsh "git checkout $CHANGE_BRANCH"
            }
        }
    }
}

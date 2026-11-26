pipeline {
    agent any
    stages {
        stage('Notify') {
            steps {
                sh 'echo "Building $CHANGE_TITLE"'
                sh '''
                    git checkout "$CHANGE_BRANCH"
                    deploy --version "$VERSION"
                '''
                powershell 'Write-Output $env:CHANGE_TITLE'
            }
        }
    }
}

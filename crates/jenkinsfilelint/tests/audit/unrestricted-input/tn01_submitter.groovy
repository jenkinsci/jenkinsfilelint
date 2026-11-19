pipeline {
    agent { label 'linux' }
    stages {
        stage('Deploy') {
            input {
                message 'Deploy to production?'
                submitter 'release-managers,alice'
            }
            steps {
                input message: 'Really?', submitter: 'release-managers'
                sh './deploy.sh production'
            }
        }
    }
}

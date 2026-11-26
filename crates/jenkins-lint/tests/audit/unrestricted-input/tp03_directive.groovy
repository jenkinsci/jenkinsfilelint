pipeline {
    agent { label 'linux' }
    stages {
        stage('Deploy') {
            input {
                message 'Deploy to production?'
                ok 'Deploy'
                submitterParameter 'APPROVER'
            }
            steps {
                sh './deploy.sh production'
            }
        }
    }
}

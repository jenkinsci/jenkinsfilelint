pipeline {
    agent { label 'linux' }
    stages {
        stage('Deploy') {
            steps {
                input 'Deploy to production?'
                sh './deploy.sh production'
            }
        }
    }
}

// An empty submitter restricts nothing.
input message: 'Proceed?', submitter: ''

pipeline {
    agent { label 'linux' }
    stages {
        stage('Deploy') {
            input {
                message 'Proceed?'
                submitter ''
            }
            steps {
                sh './deploy.sh'
            }
        }
    }
}

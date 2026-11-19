pipeline {
    agent any
    stages {
        stage('Build') {
            steps {
                'echo' 'a' 'b'
            }
        }
    }
}

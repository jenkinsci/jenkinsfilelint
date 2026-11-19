pipeline {
    agent any
    stages {
        stage('S') {
            steps {
                script {
                    foo 'a' 'b'
                    foo 'a' b 'c'
                }
            }
        }
    }
}

pipeline {
    agent any
    stages {
        stage('Build') {
            steps {
                a[0] 'x'
            }
        }
    }
}

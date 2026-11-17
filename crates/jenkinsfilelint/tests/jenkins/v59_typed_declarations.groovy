pipeline {
    agent any
    stages {
        stage('S') {
            steps {
                script {
                    String a, b = 'x'
                    int i = 0, j = 1
                    String[] parts = 'a,b'.split(',')
                    Map<String, List<Integer>> m = [:]
                    final String c = 'y'
                    def String d = 'z'
                    boolean ok
                }
            }
        }
    }
}
